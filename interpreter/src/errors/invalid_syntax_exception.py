from typing import Self

from src.errors.qlang_exception import QlangException
from src.tools.position import Position


class InvalidSyntaxException(QlangException):
    def __init__(
        self: Self,
        pos_start: Position,
        pos_end: Position,
        details: str,
        code: str,
    ) -> None:
        super().__init__(
            pos_start,
            pos_end,
            "Syntaxe non valide",
            details,
            code,
        )
