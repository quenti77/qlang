from ast import Expression

from src.nodes.node_type import NodeType


class Statement:
    kind: NodeType


class Progam(Statement):
    kind = NodeType.PROGAM
    body: list[Statement]


class BlockStatement(Statement):
    kind = NodeType.BLOCK_STATEMENT
    body: list[Statement]


class VariableDeclarationStatement(Statement):
    kind = NodeType.VARIABLE_DECLARATION_STATEMENT
    identifier: str
    value: Expression | None


class PrintStatement(Statement):
    kind = NodeType.PRINT_STATEMENT
    value: Expression


class IfStatement(Statement):
    kind = NodeType.IF_STATEMENT
    condition: Expression
    then_branch: Statement
    else_branch: Statement | None


class WhileStatement(Statement):
    kind = NodeType.WHILE_STATEMENT
    condition: Expression
    body: Statement


class ForStatement(Statement):
    kind = NodeType.FOR_STATEMENT
    identifier: str
    init: Expression
    until: Expression
    step: Expression
    body: Statement


class FunctionStatement(Statement):
    kind = NodeType.FUNCTION_STATEMENT
    identifier: str | None
    parameters: list[str]
    body: BlockStatement


class BreakStatement(Statement):
    kind = NodeType.BREAK_STATEMENT


class ContinueStatement(Statement):
    kind = NodeType.CONTINUE_STATEMENT


class ReturnStatement(Statement):
    kind = NodeType.RETURN_STATEMENT
    value: Expression
