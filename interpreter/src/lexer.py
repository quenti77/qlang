import re
from typing import Self

from src.enums.keywords import KEYWORDS
from src.enums.operators import OPERATORS
from src.enums.token_type import TOKEN_TYPE
from src.errors.illegal_char_exception import IllegalCharException
from src.errors.string_unterminated_exception import StringUnterminatedException
from src.token import Token
from src.token import create_token
from src.tools.position import Position


class Lexer:

    current_line: None | str
    position: Position

    lines: list[str]
    tokens: list[Token]
    src: list[str]
    code: str

    def __init__(self: Self) -> None:
        self.current_line = None
        self.position = Position(0, 0, 0, "")
        self.lines = []
        self.tokens = []
        self.src = []
        self.code = ""

    def get_tokens(self: Self) -> list[Token]:
        return self.tokens

    def tokenize(self: Self, input: str) -> None:
        self.__reset()
        self.code = input
        self.lines = input.split("\n")

        while self.__has_more_lines():
            self.__next_line()
            self.__tokenize_line()

        self.__push_token(TOKEN_TYPE.EOF, "")

    def __tokenize_line(self: Self) -> None:
        if self.current_line is None:
            return

        self.src = list(self.current_line)

        while self.__has_more_chars():
            if self.__process_operator():
                self.__push_token(TOKEN_TYPE.BINARY_OPERATOR, self.src.pop(0))
            elif self.src[0] == "-":
                self.__push_token(TOKEN_TYPE.UNARY_OPERATOR, self.src.pop(0))
            elif self.src[0] == "(":
                self.__push_token(TOKEN_TYPE.OPEN_PARENTHESIS, self.src.pop(0))
            elif self.src[0] == ")":
                self.__push_token(
                    TOKEN_TYPE.CLOSE_PARENTHESIS,
                    self.src.pop(0),
                )
            elif self.src[0] == "[":
                self.__push_token(TOKEN_TYPE.OPEN_BRACKET, self.src.pop(0))
            elif self.src[0] == "]":
                self.__push_token(TOKEN_TYPE.CLOSE_BRACKET, self.src.pop(0))
            elif self.src[0] == ",":
                self.__push_token(TOKEN_TYPE.COMMA, self.src.pop(0))
            elif self.__is_begin_with_logical_operator(self.src[0]):
                self.__process_logical_operator()
            elif self.src[0] == "." or self.__is_number(self.src[0]):
                self.__process_number()
            elif self.src[0] == '"':
                self.__process_string()
            elif self.__is_identifier(self.src[0], False):
                if self.__process_identifier():
                    break
            else:
                self.position.advance(False, self.src.pop(0))

    def __process_operator(self: Self) -> None:
        if self.__is_begin_with_logical_operator(self.src[0]):
            return False

        if self.src[0] not in OPERATORS:
            return False

        if self.src[0] == "-" and re.match(r"[a-zA-Z0-9]|\(|\)", self.src[1]):
            return False

        return True

    def __is_begin_with_logical_operator(self: Self, char: str) -> bool:
        return char in ["=", "!", "<", ">"]

    def __process_logical_operator(self: Self) -> None:
        current_char = self.src.pop(0)
        if self.__has_more_chars() and self.src[0] == "=":
            current_char += self.src.pop(0)

        token = TOKEN_TYPE.EQUALS if current_char == "=" else TOKEN_TYPE.BINARY_OPERATOR
        self.__push_token(token, current_char)

    def __process_number(self: Self) -> None:
        token = self.src.pop(0)
        has_dot = token == "."

        while self.__has_more_chars() and (
            self.__is_number(self.src[0]) or self.src[0] == "."
        ):
            if self.src[0] == ".":
                if has_dot:
                    pos_start = self.position.copy()
                    self.__add_col(token)
                    pos_end = self.position.copy()
                    raise IllegalCharException(pos_start, pos_end, ".", self.code)

                has_dot = True

            token += self.src.pop(0)

        self.__push_token(TOKEN_TYPE.NUMBER, token)

    def __process_string(self: Self) -> None:
        self.__eat()
        pos_start = self.position.copy()
        value = ""

        while len(self.src) == 0 or self.src[0] != '"':
            current_char = self.__eat() if len(self.src) > 0 else None
            if current_char is None:
                if not self.__has_more_lines():
                    pos_end = self.position.copy()
                    raise StringUnterminatedException(pos_start, pos_end, self.code)
                self.__next_line()
                value += "\n"
                continue

            if current_char == "\\":
                next_char = self.src.pop(0)
                if next_char == "\\":
                    value += "\\"
                elif next_char == '"':
                    value += '"'
                elif next_char == "n":
                    value += "\n"
                elif next_char == "t":
                    value += "\t"
                else:
                    self.__add_col(current_char)
                    pos_end = self.position.copy()
                    raise IllegalCharException(
                        pos_start, pos_end, f"{next_char}", self.code
                    )

                continue

            value += current_char

        pos_start.content = value
        self.tokens.append(create_token(TOKEN_TYPE.STRING, value, pos_start))
        self.__eat()

    def __process_identifier(self: Self) -> bool:
        token = self.src.pop(0)
        while self.__has_more_chars() and self.__is_identifier(self.src[0], True):
            token += self.src.pop(0)

        if token in KEYWORDS:
            self.__push_token(KEYWORDS[token], token)
        else:
            if token == "rem":
                self.__add_col(token)
                while self.__has_more_chars():
                    self.src.pop(0)
                    return True
            self.__push_token(TOKEN_TYPE.IDENTIFIER, token)

        return False

    def __eat(self: Self) -> str | None:
        current_char = self.src.pop(0)
        if current_char is None:
            self.__add_col("")
            return None

        self.position.advance(False, current_char)
        return current_char

    def __add_col(self: Self, char: str) -> None:
        self.position.advance(False, char)

    def __push_token(
        self: Self, type: TOKEN_TYPE, value: str, position: Position | None = None
    ) -> None:
        token_position = position.copy() if position else self.position.copy()
        token_position.content = value

        self.tokens.append(create_token(type, value, token_position))
        self.position.advance(False, value)

    def __is_identifier(self: Self, token: str | None, with_number: bool) -> bool:
        if token is None:
            return False

        if token == "_":
            return True

        return token.isalnum() if with_number else token.isalpha()

    def __is_number(self: Self, token: str | None) -> bool:
        return token.isdigit() if token else False

    def __has_more_chars(self: Self) -> bool:
        return len(self.src) > 0

    def __has_more_lines(self: Self) -> bool:
        return len(self.lines) > 0

    def __next_line(self: Self) -> None:
        self.position.advance(True, "")
        self.current_line = self.lines.pop(0)
        self.src = list(self.current_line) if self.current_line else []

    def __reset(self: Self) -> None:
        self.current_line = None
        self.position = Position(0, 0, 0, "")
        self.lines = []
        self.tokens = []
