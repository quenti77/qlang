from typing import TYPE_CHECKING
from typing import Self

from src.runtime.callable import Callable
from src.runtime.values import MK_NUMBER
from src.runtime.values import AlgebraicType
from src.runtime.values import AlgebraicValue


if TYPE_CHECKING:
    from src.interpreter import Interpreter


class TailleFunction(Callable):
    def arity(self: Self):
        return 1

    def call(
        self: Self, _: "Interpreter", args: list[AlgebraicValue]
    ) -> AlgebraicValue:
        data = args[0]
        if data.type == AlgebraicType.STRING:
            data_value: str = data.value  # type: ignore
            return MK_NUMBER(len(data_value))
        if data.type == AlgebraicType.ARRAY:
            data_value: list[AlgebraicValue] = data.value  # type: ignore
            return MK_NUMBER(len(data_value))
        return MK_NUMBER(0)

    def __str__(self: Self):
        return "<fonction @taille>"
