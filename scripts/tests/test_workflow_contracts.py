"""Regression contracts for context availability in the release-package workflow."""
from pathlib import Path
import re
import unittest

WORKFLOW = Path(__file__).resolve().parents[2] / ".github/workflows/ci.yml"


class WorkflowContracts(unittest.TestCase):
    def test_package_job_env_uses_only_pre_runner_context(self):
        package = WORKFLOW.read_text().split("\n  package-linux:\n", 1)[1]
        environment = re.search(r"\n    env:\n((?:      .*\n)+)", package).group(1)
        # GitHub validates job.env before a runner is assigned. runner.temp is
        # valid in step contexts, but rejects the complete workflow at this level.
        self.assertNotRegex(environment, r"\$\{\{\s*runner\.")
        self.assertIn("/tmp/gridloom-packages-${{ github.run_id }}", environment)
        self.assertIn("needs: [test-rust, build-frontend]", package)
        self.assertIn("ref: ${{ github.sha }}", package)


if __name__ == "__main__":
    unittest.main()
