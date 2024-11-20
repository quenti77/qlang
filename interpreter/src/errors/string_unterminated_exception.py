from typing import Self

from src.errors.qlang_exception import QlangException
from src.tools.position import Position


class StringUnterminatedException(QlangException):
    def __init__(
        self: Self,
        pos_start: Position,
        pos_end: Position,
        code: str,
    ) -> None:
        super().__init__(
            pos_start,
            pos_end,
            "Chaîne non terminée",
            "La chaîne n'est pas terminée",
            code,
        )
