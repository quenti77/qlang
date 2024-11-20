from typing import Self

from src.tools.position import Position


class QlangException(Exception):
    def __init__(
        self: Self,
        pos_start: Position,
        pos_end: Position,
        error_name: str,
        details: str,
        code: str,
    ) -> None:
        self.pos_start = pos_start
        self.pos_end = pos_end
        self.error_name = error_name
        self.details = details
        self.code = code

        if self.pos_end.content == "":
            self.pos_end = self.pos_end.copy()
            self.pos_end.content = "<ICI>"

        super().__init__(self.__str__())

    def __str__(self: Self) -> str:
        result = f"{self.error_name}: {self.details}\n"
        result += f"Sur la ligne {self.pos_start.line},"
        result += f"colonne {self.pos_start.column} "
        result += f"à la ligne {self.pos_end.line},"
        result += f"colonne {self.pos_end.column}\n"
        result += self.with_arrows()

        return result

    def with_arrows(self: Self) -> str:
        line = self.code.split("\n")[self.pos_start.line - 1]
        spaces = " " * (self.pos_start.column - 1)
        arrows = "^" * (self.pos_end.column - self.pos_start.column)

        return line + "\n" + spaces + arrows
