from __future__ import annotations

from typing import TYPE_CHECKING
from typing import Self


if TYPE_CHECKING:
    from src.runtime.values import RuntimeValue


class Environment:

    parent: Environment | None = None
    values: dict[str, RuntimeValue]

    def __init__(self: Self, parent: Environment | None = None) -> None:
        self.parent = parent
        self.values = {}

    def declareVariable(self: Self, name: str, value: RuntimeValue) -> RuntimeValue:
        if name in self.values:
            raise RuntimeError(f"Variable '{name}' déjà déclarée")

        self.values[name] = value
        return value

    def assignVariable(self: Self, name: str, value: RuntimeValue) -> RuntimeValue:
        try:
            env: Environment = self.resolve(name)  # type: ignore
            env.values[name] = value
        except RuntimeError as e:
            raise e

        return value

    def lookupVariable(self: Self, name: str) -> RuntimeValue:
        try:
            env: Environment = self.resolve(name)  # type: ignore
            return env.values[name]
        except RuntimeError as e:
            raise e

    def resolve(self: Self, name: str, throw_error: bool = True) -> Environment | None:
        if name in self.values:
            return self
        if self.parent is not None:
            return self.parent.resolve(name, throw_error)
        if throw_error:
            raise RuntimeError(f"Variable '{name}' non déclarée")
        return None
