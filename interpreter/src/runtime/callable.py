from abc import ABC
from abc import abstractmethod
from typing import TYPE_CHECKING
from typing import Self


if TYPE_CHECKING:
    from src.interpreter import Interpreter

from src.nodes.merged import FunctionStatement
from src.runtime.environment import Environment
from src.runtime.values import MK_NULL
from src.runtime.values import AlgebraicValue


class Callable(ABC):

    @abstractmethod
    def arity(self: Self) -> int:
        raise NotImplementedError("'arity' method must be implemented")

    @abstractmethod
    def call(self: Self, interpreter, args: list[AlgebraicValue]) -> AlgebraicValue:
        raise NotImplementedError("'call' method must be implemented")

    @abstractmethod
    def __str__(self: Self) -> str:
        raise NotImplementedError("'__str__' method must be implemented")


class QFunction(Callable):

    counter: int = 0

    def __init__(self, func: FunctionStatement, closure: Environment):
        self.func = func
        self.closure = closure

        if self.func.identifier is None:
            QFunction.counter += 1
            self.name = f"anon_{QFunction.counter}"
        else:
            self.name = self.func.identifier

    def arity(self: Self) -> int:
        return len(self.func.parameters)

    def call(
        self: Self, interpreter: "Interpreter", args: list[AlgebraicValue]
    ) -> AlgebraicValue:
        env = Environment(self.closure)

        for i, param in enumerate(self.func.parameters):
            env.declareVariable(param, args[i])

        value = interpreter.evaluate_block_statement(self.func.body, env)
        if value is AlgebraicValue:
            return value  # type: ignore

        return MK_NULL()

    def __str__(self: Self) -> str:
        return f"<fonction @{self.name}>"
