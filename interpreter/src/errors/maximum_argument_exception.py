from typing import Self

from src.errors.qlang_exception import QlangException
from src.tools.position import Position


class MaximumArgumentException(QlangException):
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
            "Nombre d'arguments maximum dépassé",
            details,
            code,
        )
