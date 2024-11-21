from dataclasses import dataclass

from src.nodes.expression import Expression
from src.nodes.node_type import NodeType


@dataclass
class Statement:
    kind: NodeType


@dataclass
class Program(Statement):
    kind = NodeType.PROGAM
    body: list[Statement]


@dataclass
class BlockStatement(Statement):
    kind = NodeType.BLOCK_STATEMENT
    body: list[Statement]


@dataclass
class FunctionStatement(Statement):
    kind = NodeType.FUNCTION_STATEMENT
    identifier: str | None
    parameters: list[str]
    body: BlockStatement


@dataclass
class VariableDeclarationStatement(Statement):
    kind = NodeType.VARIABLE_DECLARATION_STATEMENT
    identifier: str
    value: Expression | FunctionStatement | None


@dataclass
class PrintStatement(Statement):
    kind = NodeType.PRINT_STATEMENT
    value: Expression


@dataclass
class IfStatement(Statement):
    kind = NodeType.IF_STATEMENT
    condition: Expression
    then_branch: Statement
    else_branch: Statement | None


@dataclass
class WhileStatement(Statement):
    kind = NodeType.WHILE_STATEMENT
    condition: Expression
    body: Statement


@dataclass
class ForStatement(Statement):
    kind = NodeType.FOR_STATEMENT
    identifier: str
    init: Expression
    until: Expression
    step: Expression
    body: Statement


@dataclass
class BreakStatement(Statement):
    kind = NodeType.BREAK_STATEMENT


@dataclass
class ContinueStatement(Statement):
    kind = NodeType.CONTINUE_STATEMENT


@dataclass
class ReturnStatement(Statement):
    kind = NodeType.RETURN_STATEMENT
    value: Expression
