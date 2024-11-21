from typing import Self

from src.enums.token_type import TOKEN_TYPE
from src.tools.position import Position


class Token:

    def __init__(self: Self, type: TOKEN_TYPE, value: str, position: Position) -> None:
        self.type = type
        self.value = value
        self.position = position

    def __eq__(self: Self, other: object) -> bool:
        return (
            isinstance(other, Token)
            and self.type == other.type
            and self.value == other.value
            and self.position == other.position
        )

    def __str__(self: Self) -> str:
        return f"{self.type}: {self.value} at {self.position}"


def create_token(type: TOKEN_TYPE, value: str, position: Position) -> Token:
    return Token(type, value, position.copy())


def create_token_at(
    type: TOKEN_TYPE, value: str, index: int, line: int, column: int
) -> Token:
    return Token(type, value, Position(index, line, column, value))


def create_eof_token() -> Token:
    return Token(TOKEN_TYPE.EOF, "", Position(0, 0, 0, ""))
