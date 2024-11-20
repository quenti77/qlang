from enum import Enum


class UNARY_OPERATOR(Enum):
    MINUS = "-"
    NOT = "non"


class BINARY_OPERATOR(Enum):
    PLUS = "+"
    MINUS = "-"
    MULTIPLY = "*"
    DIVIDE = "/"
    MODULUS = "%"
    EQUALS_EQUALS = "=="
    NOT_EQUALS = "!="
    LESS_THAN = "<"
    LESS_THAN_EQUALS = "<="
    GREATER_THAN = ">"
    GREATER_THAN_EQUALS = ">="
    AND = "et"
    OR = "ou"


OPERATORS = [operator.value for operator in BINARY_OPERATOR]
