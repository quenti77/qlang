from dataclasses import dataclass

from src.nodes.node_type import NodeType
from src.nodes.statement import FunctionStatement
from src.nodes.statement import Statement


@dataclass
class Expression(Statement):
    pass


@dataclass
class AssignmentExpression(Expression):
    kind = NodeType.ASSIGNMENT_EXPRESSION
    assignment: Expression
    value: Expression


@dataclass
class UnaryExpression(Expression):
    kind = NodeType.UNARY_EXPRESSION
    operator: str
    value: Expression


@dataclass
class BinaryExpression(Expression):
    kind = NodeType.BINARY_EXPRESSION
    operator: str
    left: Expression
    right: Expression


@dataclass
class Identifier(Expression):
    kind = NodeType.IDENTIFIER
    identifier: str


@dataclass
class MemberExpression(Expression):
    kind = NodeType.MEMBER_EXPRESSION
    object: Expression
    property: Expression | None


@dataclass
class ArrayExpression(Expression):
    kind = NodeType.ARRAY_EXPRESSION
    elements: list[Expression]


@dataclass
class CallExpression(Expression):
    kind = NodeType.CALL_EXPRESSION
    callee: Expression
    arguments: list[Expression | FunctionStatement]


@dataclass
class Literal(Expression):
    pass


@dataclass
class NumericLiteral(Literal):
    kind = NodeType.NUMERIC_LITERAL
    value: float


@dataclass
class StringLiteral(Literal):
    kind = NodeType.STRING_LITERAL
    value: str


@dataclass
class NullLiteral(Literal):
    kind = NodeType.NULL_LITERAL
    value: "None"


@dataclass
class BooleanLiteral(Literal):
    kind = NodeType.BOOLEAN_LITERAL
    value: bool
