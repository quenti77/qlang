from dataclasses import dataclass

from src.nodes.node_type import NodeType


@dataclass
class Statement:
    kind: NodeType


@dataclass
class Expression(Statement):
    pass


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
