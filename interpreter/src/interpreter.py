from typing import Self

from src.nodes.merged import BlockStatement
from src.nodes.merged import FunctionStatement
from src.nodes.merged import Statement
from src.nodes.node_type import NodeType
from src.nodes.statement import ForStatement
from src.nodes.statement import IfStatement
from src.nodes.statement import PrintStatement
from src.nodes.statement import Program
from src.nodes.statement import ReturnStatement
from src.nodes.statement import VariableDeclarationStatement
from src.nodes.statement import WhileStatement
from src.runtime.callable import QFunction
from src.runtime.environment import Environment
from src.runtime.std import Std
from src.runtime.values import MK_BREAK
from src.runtime.values import MK_CONTINUE
from src.runtime.values import MK_FUNCTION
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
        if ast_node.kind == NodeType.PROGAM:
            return self.__evaluate_program_statement(ast_node, self.env)  # type: ignore
        if ast_node.kind == NodeType.BLOCK_STATEMENT:
            return self.evaluate_block_statement(ast_node, self.env)  # type: ignore
        if ast_node.kind == NodeType.VARIABLE_DECLARATION_STATEMENT:
            return self.__evaluate_variable_declaration_statement(ast_node)  # type: ignore
        if ast_node.kind == NodeType.PRINT_STATEMENT:
            return self.__evaluate_print_statement(ast_node)  # type: ignore

        if ast_node.kind == NodeType.IF_STATEMENT:
            return self.__evaluate_if_statement(ast_node)  # type: ignore
        if ast_node.kind == NodeType.WHILE_STATEMENT:
            return self.__evaluate_while_statement(ast_node)  # type: ignore
        if ast_node.kind == NodeType.FOR_STATEMENT:
            return self.__evaluate_for_statement(ast_node)  # type: ignore
        if ast_node.kind == NodeType.FUNCTION_STATEMENT:
            return self.__evaluate_function_statement(ast_node)  # type: ignore

        return self.__evaluate_expression(ast_node)

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

    def __evaluate_program_statement(
        self: Self, program_statement: Program
    ) -> RuntimeValue:
        last_evaluated: RuntimeValue = MK_NULL()

        for statement in program_statement.body:
            last_evaluated = self.evaluate(statement)
            if (
                last_evaluated.type == AlgebraicType.BREAK
                or last_evaluated.type == AlgebraicType.CONTINUE
                or last_evaluated.type == AlgebraicType.RETURN
            ):
                return last_evaluated

        return last_evaluated

    def __evaluate_variable_declaration_statement(
        self: Self, variable_declaration_statement: VariableDeclarationStatement
    ) -> RuntimeValue:
        if variable_declaration_statement.value is not None:
            value = self.evaluate(variable_declaration_statement.value)
            self.env.declareVariable(variable_declaration_statement.identifier, value)
            return value

        self.env.declareVariable(variable_declaration_statement.identifier, MK_NULL())
        return MK_NULL()

    def __evaluate_print_statement(
        self: Self, print_statement: PrintStatement
    ) -> RuntimeValue:
        statement = self.evaluate(print_statement.value)
        self.std_out.print(self.__runtime_string(statement))
        return MK_NULL()

    def __runtime_string(self: Self, statement: RuntimeValue) -> str:
        if not hasattr(statement, "value"):
            return str(statement.type)

        if statement.type != AlgebraicType.ARRAY:
            return str(statement.value)

        arr = statement.value
        return f"[{', '.join([self.__runtime_string(val) for val in arr])}]"

    def __evaluate_if_statement(self: Self, if_statement: IfStatement) -> RuntimeValue:
        condition = self.__to_algebraic_value(self.evaluate(if_statement.condition))

        if condition.value:
            self.env = Environment(self.env)
            result = self.evaluate(if_statement.then_branch)

            self.env = self.env.parent  # type: ignore
            return result

        if if_statement.else_branch is not None:
            self.env = Environment(self.env)
            result = self.evaluate(if_statement.else_branch)

            self.env = self.env.parent  # type: ignore
            return result

        return MK_NULL()

    def __evaluate_while_statement(
        self: Self, while_statement: WhileStatement
    ) -> RuntimeValue:
        self.env = Environment(self.env)

        while self.__to_algebraic_value(self.evaluate(while_statement.condition)).value:
            result = self.evaluate(while_statement.body)
            if result.type == AlgebraicType.BREAK:
                break
            if result.type == AlgebraicType.RETURN:
                self.env = self.env.parent  # type: ignore
                return result

        self.env = self.env.parent  # type: ignore
        return MK_NULL()

    def __evaluate_for_statement(
        self: Self, for_statement: ForStatement
    ) -> RuntimeValue:
        self.env = Environment(self.env)

        if self.env.resolve(for_statement.identifier, False) is None:
            self.env.declareVariable(for_statement.identifier, MK_NULL())

        self.env.assignVariable(
            for_statement.identifier, self.evaluate(for_statement.init)
        )

        while self.__to_algebraic_value(self.evaluate(for_statement.until)).value:
            result = self.evaluate(for_statement.body)
            if result.type == AlgebraicType.BREAK:
                break
            if result.type == AlgebraicType.RETURN:
                self.env = self.env.parent  # type: ignore
                return result
            self.evaluate(for_statement.step)

        self.env = self.env.parent  # type: ignore
        return MK_NULL()

    def __evaluate_function_statement(
        self: Self, function_statement: FunctionStatement
    ) -> RuntimeValue:
        qFunction = QFunction(function_statement, self.env)
        name = qFunction.name

        envFound = self.env.resolve(name, False)
        if envFound is None:
            self.env.declareVariable(name, MK_NULL())

        return self.env.assignVariable(name, MK_FUNCTION(qFunction))

    def __evaluate_expression(self: Self, expression: Statement) -> RuntimeValue:
        return MK_NULL()

    def __to_algebraic_value(self: Self, value: RuntimeValue) -> AlgebraicValue:
        if not hasattr(value, "value"):
            return MK_NULL()

        return value  # type: ignore
