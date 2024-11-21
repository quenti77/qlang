from src.nodes.node_type import NodeType
from src.nodes.statement import FunctionStatement
from src.nodes.statement import Statement


class Expression(Statement):
    pass


class AssignmentExpression(Expression):
    kind = NodeType.ASSIGNMENT_EXPRESSION
    assignment: Expression
    value: Expression


class UnaryExpression(Expression):
    kind = NodeType.UNARY_EXPRESSION
    operator: str
    value: Expression


class BinaryExpression(Expression):
    kind = NodeType.BINARY_EXPRESSION
    operator: str
    left: Expression
    right: Expression


class Identifier(Expression):
    kind = NodeType.IDENTIFIER
    identifier: str


class MemberExpression(Expression):
    kind = NodeType.MEMBER_EXPRESSION
    object: Expression
    property: Expression | None


class ArrayExpression(Expression):
    kind = NodeType.ARRAY_EXPRESSION
    elements: list[Expression]


class CallExpression(Expression):
    kind = NodeType.CALL_EXPRESSION
    callee: Expression
    arguments: list[Expression | FunctionStatement]


class Literal:
    kind: NodeType


class NumericLiteral(Literal):
    kind = NodeType.NUMERIC_LITERAL
    value: float


class StringLiteral(Literal):
    kind = NodeType.STRING_LITERAL
    value: str


class NullLiteral(Literal):
    kind = NodeType.NULL_LITERAL
    value: "None"


class BooleanLiteral(Literal):
    kind = NodeType.BOOLEAN_LITERAL
    value: bool
