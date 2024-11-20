from typing import Self


class Position:
    def __init__(
        self: Self,
        index: int,
        line: int,
        column: int,
        content: str,
    ) -> None:
        self.index = index
        self.line = line
        self.column = column
        self.content = content

    @property
    def finishColumn(self: Self) -> int:
        return self.column + len(self.content)

    def advance(self: Self, newLine: bool, content: str) -> Self:
        content_length = len(content) if content else 1

        self.content = content
        self.index += content_length
        self.column += content_length

        if newLine:
            self.line += 1
            self.column = 1

        return self

    def copy(self: Self) -> Self:
        return Position(self.index, self.line, self.column, self.content)

    def __eq__(self, value: object) -> bool:
        return (
            isinstance(value, Position)
            and self.index == value.index
            and self.line == value.line
            and self.column == value.column
            and self.content == value.content
        )

    def __str__(self: Self) -> str:
        return f"line {self.line}, column {self.column}, index {self.index}"
