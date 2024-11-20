from typing import Any


def assert_array_equals(
    expected: list[Any], actual: list[Any], message: str = ""
) -> None:
    if len(actual) != len(expected):
        raise AssertionError(
            f"{message} - Expected length {len(expected)}, got {len(actual)}"
        )
    for i in range(len(actual)):
        if actual[i] != expected[i]:
            raise AssertionError(f"{message} - Expected {expected[i]}, got {actual[i]}")
    return True
