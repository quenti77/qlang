from __future__ import annotations

from dataclasses import dataclass
from enum import Enum
from typing import TYPE_CHECKING


if TYPE_CHECKING:
    from src.runtime.callable import Callable


class AlgebraicType(Enum):
    NULL = "null"
    NUMBER = "number"
    BOOLEAN = "boolean"
    STRING = "string"
    BREAK = "break"
    CONTINUE = "continue"
    RETURN = "return"
    ARRAY = "array"
    FUNCTION = "function"


@dataclass
class RuntimeValue:
    type: AlgebraicType


@dataclass
class BreakValue(RuntimeValue):
    type: AlgebraicType = AlgebraicType.BREAK


@dataclass
class ContinueValue(RuntimeValue):
    type: AlgebraicType = AlgebraicType.CONTINUE


@dataclass
class AlgebraicValue(RuntimeValue):
    value: None | float | bool | str | list[AlgebraicValue] | Callable


@dataclass
class NullValue(AlgebraicValue):
    type: AlgebraicType = AlgebraicType.NULL
    value: None = None


@dataclass
class NumberValue(AlgebraicValue):
    type: AlgebraicType = AlgebraicType.NUMBER
    value: float = 0.0


@dataclass
class BooleanValue(AlgebraicValue):
    type: AlgebraicType = AlgebraicType.BOOLEAN
    value: bool = False


@dataclass
class StringValue(AlgebraicValue):
    type: AlgebraicType = AlgebraicType.STRING
    value: str = ""


@dataclass
class ReturnValue(AlgebraicValue):  # type: ignore[misc]
    type: AlgebraicType = AlgebraicType.RETURN


@dataclass
class ArrayValue(AlgebraicValue):
    type: AlgebraicType = AlgebraicType.ARRAY
    value: list[AlgebraicValue] = []


@dataclass
class FunctionValue(AlgebraicValue):
    value: Callable  # type: ignore[misc]
    type: AlgebraicType = AlgebraicType.FUNCTION


def MK_BREAK() -> BreakValue:
    return BreakValue(type=AlgebraicType.BREAK)


def MK_CONTINUE() -> ContinueValue:
    return ContinueValue(type=AlgebraicType.CONTINUE)


def MK_NULL() -> NullValue:
    return NullValue(type=AlgebraicType.NULL, value=None)


def MK_NUMBER(value: float) -> NumberValue:
    return NumberValue(type=AlgebraicType.NUMBER, value=value)


def MK_BOOLEAN(value: bool) -> BooleanValue:
    return BooleanValue(type=AlgebraicType.BOOLEAN, value=value)


def MK_STRING(value: str) -> StringValue:
    return StringValue(type=AlgebraicType.STRING, value=value)


def MK_RETURN(return_value: AlgebraicValue) -> ReturnValue:
    return ReturnValue(type=AlgebraicType.RETURN, value=return_value.value)


def MK_ARRAY(value: list[AlgebraicValue]) -> ArrayValue:
    return ArrayValue(type=AlgebraicType.ARRAY, value=value)


def MK_FUNCTION(value: Callable) -> FunctionValue:
    return FunctionValue(type=AlgebraicType.FUNCTION, value=value)


def MK_ALGEBRAIC_VALUE(
    value: None | float | bool | str | list[AlgebraicValue] | Callable,
) -> AlgebraicValue:
    if value is None:
        return MK_NULL()
    if value is float:
        f_value: float = value  # type: ignore
        return MK_NUMBER(f_value)
    if value is bool:
        return MK_BOOLEAN(bool(value))
    if value is str:
        return MK_STRING(str(value))
    if value is list:
        arr: list[AlgebraicValue] = value  # type: ignore
        return MK_ARRAY(arr.copy())
    if value is Callable:
        func: Callable = value  # type: ignore
        return MK_FUNCTION(func)
    return MK_NULL()
