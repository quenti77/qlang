from typing import Self

from src.nodes.expression import ArrayExpression
from src.nodes.expression import AssignmentExpression
from src.nodes.expression import BinaryExpression
from src.nodes.expression import BooleanLiteral
from src.nodes.expression import CallExpression
from src.nodes.expression import Identifier
from src.nodes.expression import MemberExpression
from src.nodes.expression import NumericLiteral
from src.nodes.expression import StringLiteral
from src.nodes.expression import UnaryExpression
from src.nodes.merged import BlockStatement
from src.nodes.merged import Expression
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
from src.runtime.values import MK_ALGEBRAIC_VALUE
from src.runtime.values import MK_ARRAY
from src.runtime.values import MK_BOOLEAN
from src.runtime.values import MK_BREAK
from src.runtime.values import MK_CONTINUE
from src.runtime.values import MK_FUNCTION
from src.runtime.values import MK_NULL
from src.runtime.values import MK_NUMBER
from src.runtime.values import MK_RETURN
from src.runtime.values import MK_STRING
from src.runtime.values import AlgebraicType
from src.runtime.values import AlgebraicValue
from src.runtime.values import ArrayValue
from src.runtime.values import FunctionValue
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
            return self.__evaluate_program_statement(ast_node)  # type: ignore
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

        return self.__evaluate_expression(ast_node)  # type: ignore

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

    def __evaluate_expression(self: Self, expression: Expression) -> RuntimeValue:
        if expression.kind == NodeType.ASSIGNMENT_EXPRESSION:
            return self.__evaluate_assignment_expression(expression)  # type: ignore
        if expression.kind == NodeType.UNARY_EXPRESSION:
            return self.__evaluate_unary_expression(expression)  # type: ignore
        if expression.kind == NodeType.BINARY_EXPRESSION:
            return self.__evaluate_binary_expression(expression)  # type: ignore

        if expression.kind == NodeType.ARRAY_EXPRESSION:
            return self.__evaluate_array_expression(expression)  # type: ignore
        if expression.kind == NodeType.MEMBER_EXPRESSION:
            return self.__evaluate_member_expression(expression)  # type: ignore
        if expression.kind == NodeType.CALL_EXPRESSION:
            return self.__evaluate_call_expression(expression)  # type: ignore

        if expression.kind == NodeType.IDENTIFIER:
            return self.__evaluate_identifier(expression)  # type: ignore
        if expression.kind == NodeType.NUMERIC_LITERAL:
            runtime: NumericLiteral = expression  # type: ignore
            return MK_NUMBER(runtime.value)
        if expression.kind == NodeType.STRING_LITERAL:
            runtime: StringLiteral = expression  # type: ignore
            return MK_STRING(str(runtime.value))

        if expression.kind == NodeType.BOOLEAN_LITERAL:
            runtime: BooleanLiteral = expression  # type: ignore
            return MK_BOOLEAN(bool(runtime.value))
        if expression.kind == NodeType.NULL_LITERAL:
            return MK_NULL()

        raise Exception(
            f"Impossible d'évaluer l'expression de type '{expression.kind}'"
        )

    def __evaluate_assignment_expression(
        self: Self, assignment_expression: AssignmentExpression
    ) -> RuntimeValue:
        if assignment_expression.kind == NodeType.IDENTIFIER:
            identifier: Identifier = assignment_expression  # type: ignore
            name = identifier.identifier
            return self.env.assignVariable(
                name, self.evaluate(assignment_expression.value)
            )

        if assignment_expression.kind != NodeType.MEMBER_EXPRESSION:
            raise Exception(
                f"Impossible d'assigner une valeur à une expression de type '{assignment_expression.kind}'"
            )

        member_expression: MemberExpression = assignment_expression  # type: ignore
        object = self.__to_algebraic_value(
            self.__evaluate_expression(member_expression.object)
        )

        if object.type != AlgebraicType.ARRAY:
            raise Exception("Seulements les tableaux peuvent être assignés")

        array: ArrayValue = object  # type: ignore

        if member_expression.property is None:
            evaluated_value = self.__evaluate_expression(assignment_expression.value)
            value = self.__to_algebraic_value(evaluated_value)
            array.value.append(value)  # type: ignore
            return value

        property = self.__evaluate_expression(member_expression.property)
        if (
            not isinstance(property, AlgebraicValue)
            or property.type != AlgebraicType.NUMBER
        ):
            raise Exception("Seulement les nombres peuvent être utilisés comme index")

        property_typed: NumericLiteral = property  # type: ignore
        index_value = int(property_typed.value)

        if index_value < 0 or index_value >= len(array.value):
            raise Exception(
                f"Index hors limite, l'index doit être compris entre 0 et {len(array.value) - 1}"
            )

        value = self.__to_algebraic_value(
            self.__evaluate_expression(assignment_expression.value)
        )
        array.value[index_value] = value
        return value

    def __evaluate_unary_expression(
        self: Self, expression: UnaryExpression
    ) -> RuntimeValue:
        argument = self.__to_algebraic_value(self.__evaluate_expression(expression))

        if not hasattr(argument, "value"):
            return MK_NULL()

        if expression.operator == "non":
            return MK_BOOLEAN(not argument.value)
        if expression.operator == "-":
            if argument.type != AlgebraicType.NUMBER:
                raise Exception("Seulement les nombres peuvent être négatifs")

            argument_typed: NumericLiteral = argument  # type: ignore
            return MK_NUMBER(-argument_typed.value)

        return MK_NULL()

    def __evaluate_binary_expression(
        self: Self, expression: BinaryExpression
    ) -> RuntimeValue:
        if expression.operator in ["et", "ou", "==", "!=", "<", "<=", ">", ">="]:
            return self.__evaluate_logical_expression(expression)

        left_side = self.__to_algebraic_value(
            self.__evaluate_expression(expression.left)
        )
        right_side = self.__to_algebraic_value(
            self.__evaluate_expression(expression.right)
        )

        return self.__evaluate_arithmetic_expression(
            expression.operator, left_side, right_side
        )

    def __evaluate_array_expression(
        self: Self, expression: ArrayExpression
    ) -> RuntimeValue:
        return MK_ARRAY(
            [
                self.__to_algebraic_value(self.evaluate(value))
                for value in expression.elements
            ]
        )

    def __evaluate_member_expression(
        self: Self, expression: MemberExpression
    ) -> RuntimeValue:
        object = self.__to_algebraic_value(
            self.__evaluate_expression(expression.object)
        )

        if object.type != AlgebraicType.ARRAY:
            raise Exception("Seulement les tableaux peuvent être indexés")

        array: ArrayValue = object  # type: ignore
        if expression.property is None:
            raise Exception("Les tableaux doivent être indexés")

        property = self.__evaluate_expression(expression.property)
        if (
            not isinstance(property, AlgebraicValue)
            or property.type != AlgebraicType.NUMBER
        ):
            raise Exception("Seulement les nombres peuvent être utilisés comme index")

        property_typed: NumericLiteral = property  # type: ignore
        index_value = int(property_typed.value)

        if index_value < 0 or index_value >= len(array.value):
            raise Exception(
                f"Index hors limite, l'index doit être compris entre 0 et {len(array.value) - 1}"
            )

        return array.value[index_value]  # type: ignore

    def __evaluate_call_expression(
        self: Self, expression: CallExpression
    ) -> RuntimeValue:
        func = self.__to_algebraic_value(self.__evaluate_expression(expression.callee))
        if func.type != AlgebraicType.FUNCTION:
            raise Exception("Seulement les fonctions peuvent être appelées")

        callee: FunctionValue = func  # type: ignore
        if len(expression.arguments) != callee.value.arity:
            raise Exception(
                f"Le nombre d'arguments attendu est de {callee.value.arity}, mais {len(expression.arguments)} ont été fournis"
            )

        args = [
            self.__to_algebraic_value(self.evaluate(arg))
            for arg in expression.arguments
        ]

        current_value = callee.value.call(self, args)
        if current_value.type == AlgebraicType.RETURN:
            return MK_ALGEBRAIC_VALUE(current_value.value)
        return current_value

    def __evaluate_identifier(self: Self, expression: Identifier) -> RuntimeValue:
        val = self.env.lookupVariable(expression.identifier)
        return val

    def __evaluate_logical_expression(
        self: Self, expression: BinaryExpression
    ) -> RuntimeValue:
        left_side = self.__to_algebraic_value(
            self.__evaluate_expression(expression.left)
        )
        right_side = self.__to_algebraic_value(
            self.__evaluate_expression(expression.right)
        )

        left: bool = bool(left_side.value)  # type: ignore
        right: bool = bool(right_side.value)  # type: ignore

        if expression.operator == "et":
            return MK_BOOLEAN(left and right)
        if expression.operator == "ou":
            return MK_BOOLEAN(left or right)
        if expression.operator == "==":
            return MK_BOOLEAN(left == right)
        if expression.operator == "!=":
            return MK_BOOLEAN(left != right)
        if expression.operator == "<":
            return MK_BOOLEAN(left < right)
        if expression.operator == "<=":
            return MK_BOOLEAN(left <= right)
        if expression.operator == ">":
            return MK_BOOLEAN(left > right)
        if expression.operator == ">=":
            return MK_BOOLEAN(left >= right)

        return MK_NULL()

    def __evaluate_arithmetic_expression(
        self: Self, operator: str, left: AlgebraicValue, right: AlgebraicValue
    ) -> RuntimeValue:
        if left.type == AlgebraicType.STRING or right.type == AlgebraicType.STRING:
            if operator != "+":
                raise Exception(
                    "Seulement l'opérateur '+' peut être utilisé avec des chaînes de caractères"
                )

            return MK_STRING(str(left.value) + str(right.value))

        if left.type != AlgebraicType.NUMBER or right.type != AlgebraicType.NUMBER:
            raise Exception(
                "Seulement les nombres peuvent être utilisés dans des opérations arithmétiques"
            )

        left_typed: NumericLiteral = left  # type: ignore
        right_typed: NumericLiteral = right  # type: ignore

        if operator == "+":
            return MK_NUMBER(left_typed.value + right_typed.value)
        if operator == "-":
            return MK_NUMBER(left_typed.value - right_typed.value)
        if operator == "*":
            return MK_NUMBER(left_typed.value * right_typed.value)
        if operator == "/":
            return MK_NUMBER(left_typed.value / right_typed.value)
        if operator == "%":
            return MK_NUMBER(left_typed.value % right_typed.value)

        return MK_NULL()

    def __to_algebraic_value(self: Self, value: RuntimeValue) -> AlgebraicValue:
        if not hasattr(value, "value"):
            return MK_NULL()

        return value  # type: ignore
