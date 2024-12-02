from typing import Self

from src.nodes.merged import BlockStatement
from src.nodes.merged import Statement
from src.nodes.node_type import NodeType
from src.nodes.statement import ReturnStatement
from src.runtime.environment import Environment
from src.runtime.std import Std
from src.runtime.values import MK_BREAK
from src.runtime.values import MK_CONTINUE
from src.runtime.values import MK_NULL
from src.runtime.values import MK_RETURN
from src.runtime.values import AlgebraicType
from src.runtime.values import AlgebraicValue
from src.runtime.values import RuntimeValue


class Interpreter:

    env: Environment
    std_out: Std
    std_err: Std

    def __init__(self: Self, env: Environment, std_out: Std, std_err: Std) -> None:
        self.env = env
        self.std_out = std_out
        self.std_err = std_err

    def evaluate(self: Self, ast_node: Statement) -> RuntimeValue:
        if ast_node.kind == NodeType.BLOCK_STATEMENT:
            return self.evaluate_block_statement(ast_node, self.env)  # type: ignore
        return MK_NULL()

    def evaluate_block_statement(
        self: Self, block_statement: BlockStatement, environment: Environment
    ) -> RuntimeValue:
        previous_env = self.env
        self.env = environment

        last_evaluated: RuntimeValue = MK_NULL()
        for statement in block_statement.body:
            if statement.kind == NodeType.BREAK_STATEMENT:
                self.env = previous_env
                return MK_BREAK()
            if statement.kind == NodeType.CONTINUE_STATEMENT:
                self.env = previous_env
                return MK_CONTINUE()
            if statement.kind == NodeType.RETURN_STATEMENT:
                stmt: ReturnStatement = statement  # type: ignore
                result: AlgebraicValue = self.evaluate(stmt.value)  # type: ignore

                self.env = previous_env
                return MK_RETURN(result)

            last_evaluated = self.evaluate(statement)
            if (
                last_evaluated.type == AlgebraicType.BREAK
                or last_evaluated.type == AlgebraicType.CONTINUE
                or last_evaluated.type == AlgebraicType.RETURN
            ):
                self.env = previous_env
                return last_evaluated

        return last_evaluated
