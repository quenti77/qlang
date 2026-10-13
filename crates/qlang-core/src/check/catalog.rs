//! Documentation of what the language provides on its own types, for editors.
//!
//! The signatures are not written here: the checker knows them (`builtin_method`), and a
//! test makes sure every method listed below exists there. Only the parameter names and
//! the explanation live in this file.

/// (name, parameter names, explanation)
pub type Entry = (&'static str, &'static str, &'static str);

pub const ARRAY: &[Entry] = &[
    ("len", "", "Number of elements."),
    ("is_empty", "", "`true` if there is no element."),
    ("push", "value", "Adds `value` at the end (same as `xs[] = value`)."),
    ("pop", "", "Removes and returns the last element, or `none` if the array is empty."),
    ("insert", "index, value", "Inserts `value` at `index` (0 to `len()`)."),
    ("remove", "index", "Removes and returns the element at `index`."),
    ("clear", "", "Removes every element."),
    ("contains", "value", "`true` if `value` is in the array."),
    ("index_of", "value", "Index of the first `value`, or `none`."),
    ("reverse", "", "Reverses the array in place."),
    ("join", "separator", "Joins the elements into one string."),
    ("sort", "", "Sorts the array in place (stable). Elements must be comparable."),
    ("min", "", "Smallest element, or `none` if empty."),
    ("max", "", "Largest element, or `none` if empty."),
    ("sum", "", "Sum of an array of numbers (`0` or `0.0` if empty)."),
    ("slice", "start, end", "New array of the elements from `start` (included) to `end` (excluded)."),
];

pub const MAP: &[Entry] = &[
    ("len", "", "Number of keys."),
    ("is_empty", "", "`true` if there is no key."),
    ("clear", "", "Removes every key."),
    ("has", "key", "`true` if `key` is present."),
    ("get", "key, default", "The value of `key`, or `default` if the key is absent."),
    ("remove", "key", "Removes `key` and returns its value, or `none`."),
    ("keys", "", "The keys, in insertion order."),
    ("values", "", "The values, in the same order as `keys()`."),
];

pub const STRING: &[Entry] = &[
    ("len", "", "Number of characters."),
    ("is_empty", "", "`true` for the empty string."),
    ("upper", "", "Upper-case copy."),
    ("lower", "", "Lower-case copy."),
    ("trim", "", "Copy without the spaces at both ends."),
    ("trim_start", "", "Copy without the spaces at the start."),
    ("trim_end", "", "Copy without the spaces at the end."),
    ("capitalize", "", "Copy with the first letter in upper case."),
    ("reverse", "", "The characters in the opposite order."),
    ("contains", "text", "`true` if `text` appears in the string."),
    ("starts_with", "text", "`true` if the string starts with `text`."),
    ("ends_with", "text", "`true` if the string ends with `text`."),
    ("index_of", "text", "Index of the first occurrence of `text`, or `none`."),
    ("last_index_of", "text", "Index of the last occurrence of `text`, or `none`."),
    ("count", "text", "Number of non-overlapping occurrences of `text` (not empty)."),
    ("replace", "from, to", "Replaces every occurrence of `from` with `to`."),
    ("split", "separator", "Splits the string (an empty separator gives the characters)."),
    ("repeat", "times", "The string repeated `times` times."),
    ("chars", "", "The characters, as strings of one character."),
    ("lines", "", "The lines."),
    ("words", "", "The words, separated by spaces."),
    ("pad_left", "width, fill", "Completes on the left up to `width` characters with `fill` (one letter)."),
    ("pad_right", "width, fill", "Completes on the right up to `width` characters with `fill` (one letter)."),
    ("is_digit", "", "`true` if there are only digits (false for `\"\"`)."),
    ("is_alpha", "", "`true` if there are only letters (false for `\"\"`)."),
    ("is_blank", "", "`true` if empty or only spaces."),
    ("code", "", "Unicode code of a one-character string (`\"A\".code()` is 65)."),
    ("to_int", "", "The integer written in the string, or `none`."),
    ("to_float", "", "The number written in the string, or `none`."),
    ("substring", "start, end", "Characters from `start` (included) to `end` (excluded)."),
];

pub const UUID: &[Entry] = &[
    ("version", "", "The version of the uuid (4 for `uuid.v4()`, 7 for `uuid.v7()`)."),
];

pub const ULID: &[Entry] = &[
    ("time", "", "The creation time stored in the ulid: milliseconds since 1970 (48 bits)."),
];

pub const DATETIME: &[Entry] = &[
    ("year", "", "The year (1 to 9999)."),
    ("month", "", "The month, from 1 (January) to 12."),
    ("day", "", "The day of the month, from 1."),
    ("hour", "", "The hour, from 0 to 23."),
    ("minute", "", "The minute, from 0 to 59."),
    ("second", "", "The second, from 0 to 59."),
    ("weekday", "", "The day of the week: 1 is Monday, 7 is Sunday."),
    ("ms", "", "Milliseconds since 1970-01-01 00:00:00 UTC (can be negative before 1970)."),
    ("add_years", "n", "The same moment `n` years later (`n` can be negative). 29 February becomes 28 February in a common year."),
    ("add_months", "n", "The same moment `n` months later. A day that does not exist in the new month becomes its last day (31 January + 1 month = 28 or 29 February)."),
    ("add_days", "n", "The same moment `n` days later."),
    ("add_hours", "n", "The moment `n` hours later."),
    ("add_minutes", "n", "The moment `n` minutes later."),
    ("add_seconds", "n", "The moment `n` seconds later."),
    ("start_of_day", "", "Midnight of the same day."),
    ("days_until", "other", "Number of calendar days from the day of this moment to the day of `other` (negative if `other` is earlier)."),
    ("format", "pattern", "Text following `pattern`: `YYYY` year, `MM` month, `DD` day, `HH` hour, `mm` minute, `ss` second, `SSS` milliseconds. Other characters are kept: `\"DD/MM/YYYY\"`."),
];

pub const INT: &[Entry] = &[
    ("char", "", "The one-character string whose code is this number (`97.char()` is `\"a\"`)."),
    ("abs", "", "Absolute value."),
    ("min", "other", "The smaller of the two."),
    ("max", "other", "The larger of the two."),
    ("pow", "exponent", "Same as `x ** exponent`."),
    ("sqrt", "", "Square root, as a float (error if negative)."),
];

pub const FLOAT: &[Entry] = &[
    ("abs", "", "Absolute value."),
    ("min", "other", "The smaller of the two."),
    ("max", "other", "The larger of the two."),
    ("pow", "exponent", "Same as `x ** exponent`."),
    ("sqrt", "", "Square root (error if negative)."),
    ("floor", "", "Rounds down, as an int."),
    ("ceil", "", "Rounds up, as an int."),
    ("round", "", "Rounds to the nearest, as an int."),
];

/// Static members of the built-in types: (type, kind, name, detail, explanation).
pub const STATICS: &[(&str, &str, &str, &str, &str)] = &[
    ("int", "function", "parse", "parse(text: string) -> int?", "The integer written in `text`, or `none` if it is not valid."),
    ("int", "constant", "MAX", "MAX: int", "The largest `int`."),
    ("int", "constant", "MIN", "MIN: int", "The smallest `int`."),
    ("float", "function", "parse", "parse(text: string) -> float?", "The number written in `text`, or `none` if it is not valid."),
    ("float", "constant", "PI", "PI: float", "The constant π."),
    ("float", "constant", "E", "E: float", "Euler's number."),
    ("uuid", "function", "v4", "v4() -> uuid", "A new random uuid (version 4). Needs a source of randomness."),
    ("uuid", "function", "v7", "v7() -> uuid", "A new uuid (version 7): the current time followed by random bits, so the uuids sort in creation order."),
    ("uuid", "function", "parse", "parse(text: string) -> uuid?", "The uuid written in `text` (`xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx`), or `none` if it is not valid."),
    ("uuid", "constant", "NIL", "NIL: uuid", "The uuid made only of zeros."),
    ("ulid", "function", "new", "new() -> ulid", "A new ulid: the current time (48 bits) followed by 80 random bits. Two ulids made in the same millisecond still sort in creation order."),
    ("ulid", "function", "at", "at(time: int) -> ulid", "A new ulid for the given time (milliseconds since 1970), with random bits."),
    ("ulid", "function", "parse", "parse(text: string) -> ulid?", "The ulid written in `text` (26 characters), or `none` if it is not valid."),
    ("datetime", "function", "now", "now() -> datetime", "The current moment. Needs a clock."),
    ("datetime", "function", "of", "of(year: int, month: int, day: int) -> datetime", "The moment `year`-`month`-`day` at midnight, or with the time too: `of(year, month, day, hour, minute, second)`. An error if the date does not exist."),
    ("datetime", "function", "at", "at(ms: int) -> datetime", "The moment `ms` milliseconds after 1970-01-01 00:00:00 UTC."),
    ("datetime", "function", "parse", "parse(text: string) -> datetime?", "The moment written in `text` (`2026-10-11`, `2026-10-11T14:30:00Z`, `2026-10-11 14:30`), or `none` if it is not valid."),
    ("time", "function", "now", "now() -> int", "The current time: milliseconds since 1970-01-01 00:00:00 UTC. Needs a clock."),
    ("random", "function", "int", "int(min: int, max: int) -> int", "A random integer from `min` to `max`, both included. Needs a source of randomness."),
    ("random", "function", "float", "float() -> float", "A random number from 0 (included) to 1 (excluded). Needs a source of randomness."),
    (
        "http",
        "function",
        "send",
        "send(req: Request) -> Result<Response, Error>",
        "Sends an HTTP request and waits for the response. Needs network access. A connection failure is `err`, not a crash.",
    ),
];

/// Built-in functions: (name, signature, explanation).
pub const FUNCTIONS: &[(&str, &str, &str)] = &[
    ("print", "print(value)", "Shows `value` followed by a new line (`print()` prints an empty line)."),
    ("write", "write(value)", "Shows `value` without a new line."),
    ("read", "read() -> string", "Reads a line and returns it without the new line. Not available everywhere (a web server has no keyboard)."),
    ("panic", "panic(message: string)", "Stops the program with an error."),
    ("assert", "assert(condition: bool, message?: string)", "Stops the program if `condition` is false."),
];

pub fn methods(owner: &str) -> &'static [Entry] {
    match owner {
        "array" => ARRAY,
        "map" => MAP,
        "string" => STRING,
        "uuid" => UUID,
        "ulid" => ULID,
        "datetime" => DATETIME,
        "int" => INT,
        "float" => FLOAT,
        _ => &[],
    }
}
