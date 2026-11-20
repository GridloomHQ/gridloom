export type FunctionCategory = 'Math' | 'Statistics' | 'Logic' | 'Text' | 'Dates' | 'Lookup & arrays' | 'Financial' | 'SQL';
export const FUNCTION_CATEGORIES: readonly FunctionCategory[] = ['Math', 'Statistics', 'Logic', 'Text', 'Dates', 'Lookup & arrays', 'Financial', 'SQL'];

type FunctionEntry = readonly [names: string, parameters: string, description: string, returns: string, example: string];
const categories: Record<FunctionCategory, readonly FunctionEntry[]> = {
  Math: [
    ['SUM', 'number, [number, …]', 'Add numbers or values in ranges.', 'The total.', 'SUM(2, 3) → 5'],
    ['SUMIF', 'range, criteria, [sum_range]', 'Add values when a corresponding cell meets one condition.', 'The sum of matched cells.', 'SUMIF(A2:A10, ">0", B2:B10)'],
    ['SUMIFS', 'sum_range, criteria_range, criteria, [criteria_range, criteria, …]', 'Add values matching all criteria.', 'The sum of matched cells.', 'SUMIFS(C2:C10, A2:A10, "Open", B2:B10, ">0")'],
    ['SUMPRODUCT', 'array, [array, …]', 'Multiply corresponding entries before adding the products.', 'The sum of products. Input arrays must have matching shapes.', 'SUMPRODUCT(A2:A10, B2:B10)'],
    ['PRODUCT', 'number, [number, …]', 'Multiply numbers or numeric values in ranges.', 'The product.', 'PRODUCT(2, 3, 4) → 24'],
    ['ABS', 'number', 'Remove a number’s sign.', 'A non-negative number.', 'ABS(-7) → 7'],
    ['SIGN', 'number', 'Determine whether a number is positive, zero or negative.', '1, 0 or -1.', 'SIGN(-7) → -1'],
    ['MOD', 'number, divisor', 'Find the remainder after division.', 'A remainder with the divisor’s sign, or #DIV/0! for a zero divisor.', 'MOD(17, 5) → 2'],
    ['POWER', 'number, exponent', 'Raise a number to a power.', 'The calculated power.', 'POWER(2, 3) → 8'],
    ['SQRT', 'number', 'Find the square root.', 'The non-negative root, or an error for a negative input.', 'SQRT(25) → 5'],
    ['ROUND', 'number, [digits]', 'Round to the nearest value at the specified precision.', 'The rounded number; ties round away from zero.', 'ROUND(1.25, 1) → 1.3'],
    ['ROUNDUP', 'number, [digits]', 'Round away from zero at the specified precision.', 'The rounded number.', 'ROUNDUP(-1.21, 1) → -1.3'],
    ['ROUNDDOWN', 'number, [digits]', 'Round toward zero at the specified precision.', 'The rounded number.', 'ROUNDDOWN(-1.29, 1) → -1.2'],
    ['INT', 'number', 'Round down to an integer.', 'The greatest integer no larger than the number.', 'INT(-1.2) → -2'],
    ['TRUNC', 'number, [digits]', 'Discard digits beyond the specified place without rounding away from zero.', 'The truncated number.', 'TRUNC(-1.29, 1) → -1.2'],
    ['CEILING.MATH|CEILING', 'number, [significance], [mode]', 'Round to a multiple, upward by default.', 'The rounded multiple; nonzero mode rounds negatives away from zero.', 'CEILING.MATH(13, 5) → 15'],
    ['FLOOR.MATH|FLOOR', 'number, [significance], [mode]', 'Round to a multiple, downward by default.', 'The rounded multiple; nonzero mode rounds negatives toward zero.', 'FLOOR.MATH(13, 5) → 10'],
  ],
  Statistics: [
    ['AVERAGE', 'number, [number, …]', 'Find the arithmetic mean.', 'The mean of the numeric inputs.', 'AVERAGE(2, 4) → 3'],
    ['AVERAGEIF', 'range, criteria, [average_range]', 'Average values when a corresponding cell meets one condition.', 'The mean of matched numeric values.', 'AVERAGEIF(A2:A10, ">0", B2:B10)'],
    ['AVERAGEIFS', 'average_range, criteria_range, criteria, [criteria_range, criteria, …]', 'Average values meeting all criteria.', 'The mean of matched numeric values.', 'AVERAGEIFS(C2:C10, A2:A10, "Open")'],
    ['COUNT', 'value, [value, …]', 'Count numbers, dates and timestamps, not text or blanks.', 'The number of numbers, dates and timestamps.', 'COUNT(1, "x", 2) → 2'],
    ['COUNTA', 'value, [value, …]', 'Count values that are not blank.', 'The number of nonblank values.', 'COUNTA(A2:A10)'],
    ['COUNTBLANK', 'value, [value, …]', 'Count blank values and empty text.', 'The number of blanks and empty strings.', 'COUNTBLANK(A2:A10)'],
    ['COUNTIF', 'range, criteria', 'Count values that meet one condition.', 'The number of matching cells.', 'COUNTIF(A2:A10, "Open")'],
    ['COUNTIFS', 'criteria_range, criteria, [criteria_range, criteria, …]', 'Count positions meeting all conditions.', 'The number of matching positions.', 'COUNTIFS(A2:A10, "Open", B2:B10, ">0")'],
    ['MIN', 'number, [number, …]', 'Find the smallest numeric value.', 'The minimum.', 'MIN(4, 2, 9) → 2'],
    ['MAX', 'number, [number, …]', 'Find the largest numeric value.', 'The maximum.', 'MAX(4, 2, 9) → 9'],
    ['MEDIAN', 'number, [number, …]', 'Find the middle after ordering the numerical values.', 'The middle value, or the mean of the two middle values.', 'MEDIAN(4, 2, 9) → 4'],
    ['STDEV.S|STDEV', 'number, [number, …]', 'Estimate standard deviation for a sample.', 'The sample standard deviation.', 'STDEV.S(1, 2, 3) → 1'],
    ['STDEV.P|STDEVP', 'number, [number, …]', 'Find standard deviation for an entire population.', 'The population standard deviation.', 'STDEV.P(2, 4) → 1'],
    ['VAR.S|VAR', 'number, [number, …]', 'Estimate variance for a sample.', 'The sample variance.', 'VAR.S(1, 2, 3) → 1'],
    ['VAR.P|VARP', 'number, [number, …]', 'Find variance for an entire population.', 'The population variance.', 'VAR.P(2, 4) → 1'],
  ],
  Logic: [
    ['IF', 'condition, value_if_true, [value_if_false]', 'Choose a result based on one condition.', 'The result for the true or false branch.', 'IF(2 > 1, "yes", "no") → yes'],
    ['IFS', 'condition, value_if_true, [condition, value_if_true, …]', 'Try conditions in order.', 'The result for the first true condition, or #N/A if none match.', 'IFS(2 > 3, "high", 2 > 1, "low") → low'],
    ['AND', 'condition, [condition, …]', 'Check whether every condition is true.', 'TRUE if all conditions are true, otherwise FALSE.', 'AND(2 > 1, 3 > 2) → TRUE'],
    ['OR', 'condition, [condition, …]', 'Check whether any condition is true.', 'TRUE if at least one condition is true, otherwise FALSE.', 'OR(2 > 1, 2 > 3) → TRUE'],
    ['NOT', 'condition', 'Reverse a logical value.', 'TRUE for false input, or FALSE for true input.', 'NOT(2 > 1) → FALSE'],
    ['IFERROR', 'value, fallback', 'Substitute a result if the first input is an error.', 'The original value or the fallback.', 'IFERROR(1 / 0, 0) → 0'],
    ['IFNA', 'value, fallback', 'Substitute a result for #N/A only.', 'The fallback for #N/A; otherwise the original value or error.', 'IFNA(MATCH("missing", A2:A10, 0), "Not found")'],
    ['ISBLANK', 'value', 'Test whether a value is blank.', 'TRUE for a blank value, otherwise FALSE.', 'ISBLANK(A2)'],
    ['ISNUMBER', 'value', 'Test whether a value is numeric.', 'TRUE for a number, otherwise FALSE.', 'ISNUMBER("12") → FALSE'],
    ['ISTEXT', 'value', 'Test whether a value is text.', 'TRUE for text, otherwise FALSE.', 'ISTEXT("12") → TRUE'],
    ['ISERROR', 'value', 'Test whether a value is an error.', 'TRUE for an error, otherwise FALSE.', 'ISERROR(1 / 0) → TRUE'],
    ['ISNA', 'value', 'Test specifically for #N/A.', 'TRUE for #N/A, otherwise FALSE.', 'ISNA(1 / 0) → FALSE'],
  ],
  Text: [
    ['CONCAT|CONCATENATE', 'text, [text, …]', 'Join pieces of text without a separator.', 'The combined text.', 'CONCAT("North", "west") → Northwest'],
    ['TEXTJOIN', 'delimiter, ignore_empty, text, [text, …]', 'Join text using a separator.', 'The combined text, skipping empty entries if requested.', 'TEXTJOIN("-", TRUE, "A", "B") → A-B'],
    ['LEFT', 'text, [count]', 'Take characters from the beginning.', 'The leading characters.', 'LEFT("North", 2) → No'],
    ['RIGHT', 'text, [count]', 'Take characters from the end.', 'The trailing characters.', 'RIGHT("North", 2) → th'],
    ['MID', 'text, start, count', 'Take characters from a given position.', 'The requested substring.', 'MID("North", 2, 3) → ort'],
    ['LEN', 'text', 'Count the characters in text.', 'The character count.', 'LEN("North") → 5'],
    ['TRIM', 'text', 'Remove leading and trailing whitespace and collapse repeated whitespace.', 'The text with normalized spaces.', 'TRIM("  North   West ") → North West'],
    ['CLEAN', 'text', 'Remove control characters.', 'The cleaned text.', 'CLEAN(A2)'],
    ['UPPER', 'text', 'Change letters to uppercase.', 'Uppercase text.', 'UPPER("North") → NORTH'],
    ['LOWER', 'text', 'Change letters to lowercase.', 'Lowercase text.', 'LOWER("North") → north'],
    ['SUBSTITUTE', 'text, old_text, new_text, [occurrence]', 'Replace matches of a particular substring.', 'The updated text.', 'SUBSTITUTE("a-a", "a", "b", 2) → a-b'],
    ['REPLACE', 'text, start, count, new_text', 'Replace a specified span by character position.', 'The updated text.', 'REPLACE("North", 1, 2, "Ea") → Earth'],
    ['FIND', 'find_text, within_text, [start]', 'Find case-sensitive text.', 'The first matching position, starting at 1, or an error if there is no match.', 'FIND("r", "North") → 3'],
    ['SEARCH', 'find_text, within_text, [start]', 'Find text without matching case.', 'The first matching position, starting at 1, or an error if there is no match.', 'SEARCH("R", "North") → 3'],
    ['VALUE', 'text', 'Convert numeric text to a number.', 'The parsed number, or an error if it cannot be parsed.', 'VALUE("12.5") → 12.5'],
    ['TEXT', 'value, format', 'Format a value using a supported decimal, percentage or date pattern.', 'Formatted text rather than a numeric or date value.', 'TEXT(1.2, "0.00") → 1.20'],
  ],
  Dates: [
    ['DATE', 'year, month, day', 'Build a date from its parts.', 'A date.', 'DATE(2024, 2, 29) → 2024-02-29'],
    ['YEAR', 'date', 'Extract the calendar year.', 'The year number.', 'YEAR(DATE(2024, 2, 29)) → 2024'],
    ['MONTH', 'date', 'Extract the calendar month.', 'A month number between 1 and 12.', 'MONTH(DATE(2024, 2, 29)) → 2'],
    ['DAY', 'date', 'Extract the day of the month.', 'A day number between 1 and 31.', 'DAY(DATE(2024, 2, 29)) → 29'],
    ['TODAY', '', 'Read the current date in UTC.', 'A date.', 'TODAY()'],
    ['NOW', '', 'Read the current date and time in UTC.', 'A timestamp.', 'NOW()'],
    ['EDATE', 'start_date, months', 'Shift a date by months.', 'A date, adjusted to the last valid day if needed.', 'EDATE(DATE(2024, 1, 31), 1) → 2024-02-29'],
    ['EOMONTH', 'start_date, months', 'Find the last day of an offset month.', 'A month-end date.', 'EOMONTH(DATE(2024, 1, 1), 1) → 2024-02-29'],
    ['DAYS', 'end_date, start_date', 'Subtract one date from another.', 'The signed number of calendar days.', 'DAYS(DATE(2024, 3, 1), DATE(2024, 2, 29)) → 1'],
    ['WEEKDAY', 'date, [weekday_scheme]', 'Find the day of the week as a number.', 'The weekday number in the selected numbering scheme.', 'WEEKDAY(DATE(2024, 1, 1), 2) → 1'],
    ['NETWORKDAYS', 'start_date, end_date, [holidays]', 'Count workdays between two dates, inclusive.', 'The number of weekdays excluding listed holidays.', 'NETWORKDAYS(DATE(2024, 1, 1), DATE(2024, 1, 5)) → 5'],
    ['WORKDAY', 'start_date, workdays, [holidays]', 'Move a date by a number of workdays.', 'A date skipping weekends and listed holidays.', 'WORKDAY(DATE(2024, 1, 5), 1) → 2024-01-08'],
  ],
  'Lookup & arrays': [
    ['XLOOKUP', 'lookup_value, lookup_vector, return_array, [if_not_found], [match_mode], [search_mode]', 'Find a match in one row or column and return the corresponding result.', 'A value or a spilling row or column; the fallback or #N/A on no match.', 'XLOOKUP("A-01", A2:A10, B2:D10, "Not found")'],
    ['VLOOKUP', 'lookup_value, table, column, [approximate]', 'Search the first column of a table.', 'The value from the requested column of the matched row, or #N/A.', 'VLOOKUP("A-01", A2:C10, 3, FALSE)'],
    ['INDEX', 'array, row, [column]', 'Retrieve a position in a range or array.', 'The requested entry; 0 can select a whole row or column.', 'INDEX(A2:C10, 2, 3)'],
    ['MATCH', 'lookup_value, lookup_vector, [match_type]', 'Find a value’s position in a range.', 'A 1-based position, or #N/A if no value qualifies.', 'MATCH("A-01", A2:A10, 0)'],
    ['FILTER', 'array, include, [if_empty]', 'Keep rows flagged by a logical mask.', 'A spilling array of matched rows, or the fallback if empty.', 'FILTER(A2:C10, D2:D10, "No rows")'],
    ['SORT', 'array, [sort_column], [sort_order]', 'Sort rows on one column without altering the source.', 'A spilling array of sorted rows.', 'SORT(A2:C10, 2, -1)'],
    ['UNIQUE', 'array', 'Keep the first row for each distinct set of displayed values.', 'A spilling array without duplicate rows.', 'UNIQUE(A2:B10)'],
    ['SEQUENCE', 'rows, [columns], [sequence_start], [step]', 'Build a series with a fixed step.', 'A spilling array of sequential numbers.', 'SEQUENCE(3) → 1, 2, 3 in a column'],
    ['TRANSPOSE', 'array', 'Swap rows and columns.', 'A spilling array in the opposite orientation.', 'TRANSPOSE(A1:C1)'],
  ],
  Financial: [
    ['PMT', 'rate, periods, present_value, [future_value], [payment_type]', 'Calculate a fixed periodic payment.', 'A signed periodic payment.', 'PMT(0, 10, 1000) → -100'],
    ['PV', 'rate, periods, payment, [future_value], [payment_type]', 'Calculate the present value of fixed payments.', 'A signed present value.', 'PV(0, 10, -100) → 1000'],
    ['FV', 'rate, periods, payment, [present_value], [payment_type]', 'Calculate the future value of fixed payments.', 'A signed future value.', 'FV(0, 10, -100) → 1000'],
    ['NPV', 'rate, cash_flow, [cash_flow, …]', 'Discount future periodic cash flows.', 'Their net present value; the first flow is one period from now.', 'NPV(0.1, 110) → 100'],
    ['IRR', 'cash_flows, [guess]', 'Find the rate at which periodic net cash flows have zero net present value.', 'The estimated internal rate of return as a decimal rate.', 'IRR(A2:A10)'],
  ],
  SQL: [
    ['SQLVALUE', 'connection, query, [parameter, …]', 'Run a read-only, parameterized query against a workbook connection.', 'The single value, NULL for no rows, or #VALUE! if more than one cell is returned.', 'SQLVALUE("LocalDB", "SELECT SUM(amount) FROM invoices")'],
    ['SQLTABLE', 'connection, query, [parameter, …]', 'Run a read-only, parameterized tabular query against a workbook connection.', 'A spilling table of query results.', 'SQLTABLE("LocalDB", "SELECT id, amount FROM invoices")'],
  ],
};

const argumentDetails: Record<string, string> = {
  number: 'A number or reference; aggregate functions also accept ranges.',
  value: 'A value or cell reference. Counting functions also accept ranges.',
  range: 'The cells to check against the condition.',
  criteria: 'A value or expression such as "Open" or ">0".',
  sum_range: 'The corresponding cells to add; SUMIF uses range itself if omitted.',
  average_range: 'The corresponding cells to average; AVERAGEIF uses range itself if omitted.',
  criteria_range: 'A range to test. Keep criteria ranges aligned with the result range.',
  divisor: 'A nonzero number to divide by.',
  exponent: 'The power to raise the base to.',
  digits: 'Decimal places; omit for zero. Negative digits round left of the decimal point.',
  significance: 'The size of the multiple; defaults to 1. The magnitude is used.',
  mode: 'Use 0 (default) or a nonzero value to change how negative numbers are rounded.',
  condition: 'An expression that can be evaluated as true or false.',
  value_if_true: 'The result when the condition is true.',
  value_if_false: 'The result when the condition is false.',
  fallback: 'The value to use instead of the specified error.',
  text: 'Text or a reference to text.',
  delimiter: 'The text inserted between joined parts.',
  ignore_empty: 'TRUE to skip empty text; FALSE to retain empty entries.',
  count: 'The number of characters; LEFT and RIGHT default to 1.',
  start: 'A 1-based character position; FIND and SEARCH start at 1 if omitted.',
  old_text: 'The text to find and replace.',
  new_text: 'The replacement text.',
  occurrence: 'A 1-based occurrence number; omit to replace all matches.',
  find_text: 'The substring to locate.',
  within_text: 'The text to search.',
  format: 'A pattern such as "0.00", "0%" or "yyyy-mm-dd".',
  year: 'The calendar year.',
  month: 'The calendar month number.',
  day: 'The day of the month.',
  date: 'A date value or date reference.',
  start_date: 'The starting date.',
  end_date: 'The ending date.',
  months: 'The number of months to move forward or backward.',
  weekday_scheme: '1: Sunday=1 (default). 2: Monday=1. 3: Monday=0.',
  holidays: 'A range of additional dates that are not workdays.',
  workdays: 'How many workdays to move forward or backward.',
  lookup_value: 'The item to find.',
  lookup_vector: 'A row or column to search.',
  return_array: 'The values aligned to the lookup row or column.',
  if_not_found: 'The value to use when no match is found.',
  match_mode: '0 for exact (default), -1 for exact or next smaller, 1 for exact or next larger.',
  search_mode: '1 searches first to last (default), -1 reverses the search.',
  table: 'The table range whose first column is searched.',
  column: 'The 1-based column position. INDEX defaults to 1; use 0 to select the whole row.',
  approximate: 'TRUE (default) for an approximate match with ascending keys; FALSE for exact.',
  array: 'A range or array of cells.',
  row: 'The 1-based row position. For INDEX, 0 selects the whole column.',
  match_type: '0 for exact, 1 for ascending approximate (default), -1 for descending approximate.',
  include: 'One logical value per row; true includes the row.',
  if_empty: 'The result if no rows match.',
  sort_column: 'The 1-based sort column; defaults to 1.',
  sort_order: '1 for ascending (default), -1 for descending.',
  rows: 'The number of rows to produce.',
  columns: 'The number of columns to produce; defaults to 1.',
  sequence_start: 'The first number; defaults to 1.',
  step: 'The difference between successive values; defaults to 1.',
  rate: 'The interest or discount rate per period, as a decimal.',
  periods: 'The total number of periods.',
  present_value: 'The amount at the start, using the cash-flow sign convention.',
  future_value: 'The target ending amount; defaults to 0.',
  payment: 'The signed periodic payment.',
  payment_type: '0 for end-of-period payments (default), 1 for beginning-of-period.',
  cash_flow: 'A signed future payment or receipt; ranges are read in order.',
  cash_flows: 'A range or array of signed flows ordered by period. Include positive and negative amounts.',
  guess: 'An initial rate estimate; defaults to 0.1.',
  connection: 'The workbook’s saved SQL connection name.',
  query: 'The SQL query as text. Use placeholders for bound parameters.',
  parameter: 'A value to bind to a SQL placeholder, in query order.',
};

export interface FunctionHelp {
  name: string;
  category: FunctionCategory;
  syntax: string;
  aliases: readonly string[];
  description: string;
  returns: string;
  example: string;
  arguments: { name: string; detail: string }[];
}

const help = new Map<string, FunctionHelp>();
for (const category of FUNCTION_CATEGORIES) {
  for (const [names, params, description, returns, example] of categories[category]) {
    const aliases = names.split('|');
    const args = [...new Set(params.match(/[a-z_]+/g) ?? [])].map(name => ({
      name, detail: (names === 'VLOOKUP' && name === 'column') ? 'The 1-based return column in the table.'
        : (names === 'INDEX' && name === 'column') ? 'The 1-based column. Defaults to 1. Use 0 to select the whole row, or use 0 for both positions to return the full array.'
        : argumentDetails[name],
    }));
    for (const name of aliases) {
      help.set(name, { name, category, aliases: aliases.filter(alias => alias !== name),
        syntax: `=${name}(${params})`, description, returns,
        example: '=' + example.replace(/^[\w.]+(?=\()/, name), arguments: args });
    }
  }
}

export function functionHelp(name: string): FunctionHelp | undefined {
  return help.get(name);
}

/** The engine list is authoritative; help is optional and must not advertise unsupported names. */
export function browserFunctions(names: readonly string[], query: string, category: FunctionCategory | 'All' | 'Other'): string[] {
  const words = query.toLowerCase().trim().split(/\s+/).filter(Boolean);
  return names.filter(name => {
    const info = functionHelp(name);
    if (category !== 'All' && category !== (info?.category ?? 'Other')) return false;
    const searchable = [name, info?.category, info?.description, info?.aliases.join(' ')].join(' ').toLowerCase();
    return words.every(word => searchable.includes(word));
  });
}
