#!/usr/bin/env python3
"""Review a flawed model, repair it, and cross-check native and agent review state."""
import argparse
from decimal import Decimal
import importlib.util
import json
from pathlib import Path
import tempfile

directory = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("mixed_save", directory / "mixed-save-smoke.py")
shared = importlib.util.module_from_spec(spec)
spec.loader.exec_module(shared)
mcp = shared.mcp
S = "Sheet1"


def status(client, cell="D1", **page):
    return client.call("review_cell", sheet=S, cell=cell, **page)


def compare(client, review_id, **args):
    return client.call("compare_review", review_id=review_id, **args)


def record(client, inspected, decision="reviewed", note="Checked exact result independently.", **kwargs):
    return client.call("record_review", revision=inspected["revision"], sheet=S,
                       cell=inspected["cell"], expected_snapshot=inspected["snapshot"],
                       decision=decision, agent_label="Forecast self-check",
                       note=note, **kwargs)


def edit(client, revision, *edits):
    return client.call("edit_cells", sheet=S, revision=revision, edits=list(edits))


def run(backend, executable, desktop=False):
    with tempfile.TemporaryDirectory(prefix="gridloom-review-mcp-exact-") as temp:
        root = Path(temp)
        name = "forecast.gridloom"
        # Independent control arithmetic in decimal, not an assertion that the
        # Gridloom worksheet (or the real-world source data) is correct.
        expected = Decimal("100.25") - Decimal("40.10")
        evidence_reference = str(root / "reference-only-not-a-file.txt")
        with mcp.Client(executable, root) as agent:
            tools = [x["name"] for x in agent.rpc("tools/list")["result"]["tools"]]
            assert tools[0] == "review_cell" and "record_review" in tools
            created = agent.call("create_workbook", file=name)
            rev = edit(agent, created["revision"], mcp.cell("A1", "decimal", "100.25"),
                        mcp.cell("B1", "decimal", "40.10"), mcp.cell("C1", "formula", "=A1+B1"),
                        mcp.cell("D1", "formula", "=C1"))["revision"]
            written = agent.call("read_cells", sheet=S, range="D1")
            typed = written["cells"][0]["value"]
            assert typed == {"type": "Decimal", "value": "140.35"} and Decimal(typed["value"]) != expected
            found = status(agent, limit=2)
            assert found["trace_page"] == {"offset": 0, "count": 2, "total": 4, "next_offset": 2}
            remainder = status(agent, offset=found["trace_page"]["next_offset"], limit=2)
            trace = found["trace"] + remainder["trace"]
            assert {x["cell"] for x in trace} == {"A1", "B1", "C1", "D1"}
            assert next(x for x in trace if x["cell"] == "C1")["formula"] == "=A1+B1"
            assert found["can_mark_reviewed"], "A calculable but wrong business result is not automatically caught"
            question = record(agent, found, decision="question",
                              note="Independent control is 60.15, not 140.35. Costs were added.",
                              evidence=[{"label":"Fixture evidence reference","reference":evidence_reference}])
            qid = question["record"]["review_id"]
            assert question["dirty"] and not question["saved"]
            assert question["record"]["status"] == "question_open"
            assert len(agent.call("list_reviews", status="open_questions")["records"]) == 1
            blocked = status(agent)
            assert not blocked["can_mark_reviewed"] and blocked["open_questions"] == 1
            record(agent, blocked, error="Resolve the open questions")
            record(agent, blocked, note=" ", error="Record the check")
            agent.call("list_reviews", status="open_questions", limit=25, error="between 1 and 24")
            assert status(agent)["revision"] == question["revision"]
            fixed = edit(agent, question["revision"], mcp.cell("C1", "formula", "=A1-B1"))
            exact = agent.call("read_cells", sheet=S, range="D1")["cells"][0]["value"]
            assert Decimal(exact["value"]) == expected
            assert agent.call("validate_cells", sheet=S, range="A1:D1")["valid"]
            current = status(agent)
            assert current["revision"] == fixed["revision"] and current["snapshot"] != found["snapshot"]
            agent.call("record_review", revision=current["revision"], sheet=S, cell="D1",
                       expected_snapshot=found["snapshot"], agent_label="Fixture",
                       decision="reviewed", note="stale", error="calculation changed")
            agent.call("transition_question", revision=question["revision"], review_id=qid,
                       expected_snapshot=current["snapshot"], expected_last_event_id=None,
                       agent_label="Fixture", action="resolve", note="stale", error="Stale revision")
            resolved = agent.call("transition_question", revision=current["revision"], review_id=qid,
                                  expected_snapshot=current["snapshot"], expected_last_event_id=None,
                                  agent_label="Forecast self-check", action="resolve", note="Corrected sign and reran independent control.")
            assert resolved["record"]["status"] == "question_resolved"
            assert status(agent)["status"] == "not_reviewed", "Resolution must not grant review"
            agent.call("transition_question", revision=resolved["revision"], review_id=qid,
                       expected_snapshot=current["snapshot"], expected_last_event_id=None,
                       agent_label="Fixture", action="reopen", note="stale", error="Question history changed")
            reopened = agent.call("transition_question", revision=resolved["revision"], review_id=qid,
                                  expected_snapshot=current["snapshot"], expected_last_event_id=resolved["record"]["last_question_event_id"],
                                  agent_label="Forecast self-check", action="reopen", note="Double-check the sign against the control.")
            assert status(agent)["status"] == "open_questions"
            agent.call("transition_question", revision=reopened["revision"], review_id=qid,
                       expected_snapshot=current["snapshot"], expected_last_event_id=reopened["record"]["last_question_event_id"],
                       agent_label="Forecast self-check", action="resolve", note="Rechecked sign and amount against original inputs.")
            approval = record(agent, status(agent), note="Independent decimal control 100.25 - 40.10 = 60.15; traced both inputs.")
            rid = approval["record"]["review_id"]
            assert approval["record"]["recorded_by"].startswith("AI-assisted (MCP): ")
            assert status(agent)["status"] == "review_current"
            q = agent.call("get_review", review_id=qid, limit=1)
            assert q["evidence_references"][0] == {"label":"Fixture evidence reference",
                                                   "reference":evidence_reference, "verified":False}
            assert q["pages"]["events"]["total"] == 3 and q["pages"]["events"]["next_offset"] == 1
            assert q["question_events"][0]["ai_assisted_mcp"]
            assert not Path(evidence_reference).exists()
            # An unrelated edit must not reopen review. A transitive input does.
            rev = edit(agent, approval["revision"], mcp.cell("H1", "integer", "9"))["revision"]
            assert status(agent)["status"] == "review_current"
            rev = edit(agent, rev, mcp.cell("B1", "decimal", "45.10"))["revision"]
            assert status(agent)["status"] == "review_stale"
            changes = compare(agent, rid)["changes"]
            diff = next(x for x in changes if x["category"] == "input" and x["cell"] == "B1")
            assert diff["before"]["value"]["value"] == "40.10" and diff["after"]["value"]["value"] == "45.10"
            assert agent.call("list_reviews", status="open_questions")["records"] == [], "Input drift invalidates a review, but does not reopen a resolved question event"
            rev = edit(agent, rev, mcp.cell("B1", "decimal", "40.10"))["revision"]
            restored = status(agent)
            assert restored["snapshot"] == approval["record"]["snapshot"] and restored["status"] == "review_stale", "Invalidation must be sticky"
            revived = record(agent, restored, note="Rechecked the restored original result.")
            rev = edit(agent, revived["revision"], mcp.cell("C1", "formula", "=A1-B1+0"))["revision"]
            assert status(agent)["status"] == "review_stale", "Same-value formula edit must invalidate"
            diff = compare(agent, revived["record"]["review_id"])["changes"]
            assert any(x["category"] == "formula" and x["cell"] == "C1" for x in diff)
            assert agent.call("list_reviews", status="stale", limit=1)["page"]["next_offset"] == 1
            # Known invalid calculations cannot generate a reviewed record.
            rev = edit(agent, rev, mcp.cell("K1", "formula", "=1/0"))["revision"]
            blocked = status(agent, "K1")
            assert not blocked["can_mark_reviewed"]
            record(agent, blocked, error="cannot be reviewed")
            question_error = record(agent, blocked, decision="question", note="Investigate division by zero.")
            saved = agent.call("save_workbook", revision=question_error["revision"])
            assert not saved["dirty"] and (root / name).is_file()

        # The real native backend reads agent records and writes back new
        # evidence with genuine uploaded bytes (agent only receives metadata).
        with shared.backend(backend, root, desktop) as native:
            assert native("workbook/open", path=str(root / name))[0] == 200
            _, report = native("cells/explain", sheet_name=S, row=0, col=3)
            assert report["review"]["invalidated"] and report["review"]["reviewer"].startswith("AI-assisted (MCP):")
            attach = list(b"Known private attachment content, retained locally")
            payload = {
                "sheet_name": S, "row": 0, "col": 3, "expected_snapshot": report["snapshot"],
                "decision": "Reviewed", "reviewer": "Native fixture", "note": "Checked fixture",
                "evidence": [{"label": "Native note", "reference": "Fixture document page 1"}],
            }
            code, latest = native("reviews/attached", review=payload, attachments=[
                {"filename":"fixture.txt","media_type":"text/plain","bytes":attach},
            ])
            assert code == 200, latest
            assert native("workbook/save", path=str(root / name))[0] == 200
            with mcp.Client(executable, root) as agent:
                rev = agent.call("open_workbook", file=name)["revision"]
                now = status(agent)
                assert now["status"] == "review_current" and not now["latest_review"]["ai_assisted_mcp"]
                info = agent.call("get_review", review_id=latest["id"])
                assert info["attachments_metadata"][0]["bytes_retained"] == len(attach)
                assert info["attachments_metadata"][0]["contents_verified"] is False
                assert "Known private attachment content" not in json.dumps(info) and '"bytes":' not in json.dumps(info)
                native_change = compare(agent, rid, after_review_id=latest["id"])["changes"]
                assert any(x["category"]=="evidence" for x in native_change)
                assert "Known private attachment content" not in json.dumps(native_change) and '"bytes":' not in json.dumps(native_change)
                assert native("cells/set", sheet_name=S, row=0,col=1,raw_input="41.10")[0] == 200
                assert native("workbook/save", path=str(root/name))[0] == 200
                agent.call("save_workbook", revision=rev, error="changed on disk")
            with mcp.Client(executable, root, read_only=True) as inspect:
                tools = {x["name"] for x in inspect.rpc("tools/list")["result"]["tools"]}
                assert {"review_cell","list_reviews","get_review","compare_review"} <= tools
                assert not tools & {"record_review","transition_question"}
                inspect.call("open_workbook", file=name)
                assert status(inspect)["status"] == "review_stale"
                assert inspect.call("list_reviews", sheet=S, cell="D1")["page"]["total"] == 4

    print("PASS: agent independent exact control, mistaken formula/question, repair and re-review, stale rejection, persistence, sticky transitive invalidation, and native attachment metadata interoperability.")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("backend", type=Path)
    parser.add_argument("mcp", type=Path)
    parser.add_argument("--desktop", action="store_true")
    args = parser.parse_args()
    run(str(args.backend.resolve()), str(args.mcp.resolve()), args.desktop)
