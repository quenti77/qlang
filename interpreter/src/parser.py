from typing import Self

from src.enums.keywords import find_keyword_from_token
from src.enums.token_type import TOKEN_TYPE
from src.errors.invalid_syntax_exception import InvalidSyntaxException
from src.nodes.expression import ArrayExpression
from src.nodes.expression import AssignmentExpression
from src.nodes.expression import BinaryExpression
from src.nodes.expression import BooleanLiteral
from src.nodes.expression import CallExpression
from src.nodes.expression import Identifier
from src.nodes.expression import MemberExpression
from src.nodes.expression import NullLiteral
from src.nodes.expression import NumericLiteral
from src.nodes.expression import StringLiteral
from src.nodes.expression import UnaryExpression
from src.nodes.merged import BlockStatement
from src.nodes.merged import Expression
from src.nodes.merged import FunctionStatement
from src.nodes.merged import Statement
from src.nodes.node_type import NodeType
from src.nodes.statement import BreakStatement
from src.nodes.statement import ContinueStatement
from src.nodes.statement import ForStatement
from src.nodes.statement import IfStatement
from src.nodes.statement import PrintStatement
from src.nodes.statement import Program
from src.nodes.statement import ReturnStatement
from src.nodes.statement import VariableDeclarationStatement
from src.nodes.statement import WhileStatement
from src.token import Token
from src.token import create_eof_token
from src.tools.position import Position


class Parser:

    code: str
    tokens: list[Token]
    previous_token: Token | None

    def __init__(self: Self) -> None:
        self.code = ""
        self.tokens = []
        self.previous_token = None

    def setTokens(self: Self, tokens: list[Token], code: str) -> None:
        self.tokens = tokens
        self.code = code

    def make_ast(self: Self) -> Program:
        program: Program = Program(
            kind=NodeType.PROGAM,
            body=[],
        )

        while self.__is_eof() is False:
            statement = self.__parse_statement()
            program.body.append(statement)

        return program

    def __parse_statement(self: Self) -> Statement:
        current_token = self.__at()

        if current_token.type == TOKEN_TYPE.LET:
            return self.__parse_variable_declaration_statement()
        elif current_token.type == TOKEN_TYPE.PRINT:
            return self.__parse_print_statement()
        elif current_token.type == TOKEN_TYPE.IF:
            return self.__parse_if_statement()
        elif current_token.type == TOKEN_TYPE.WHILE:
            return self.__parse_while_statement()
        elif current_token.type == TOKEN_TYPE.FOR:
            return self.__parse_for_statement()
        elif current_token.type == TOKEN_TYPE.FUNCTION:
            return self.__parse_function_declaration_statement()
        elif current_token.type == TOKEN_TYPE.BREAK:
            self.__eat()
            return BreakStatement(kind=NodeType.BREAK_STATEMENT)
        elif current_token.type == TOKEN_TYPE.CONTINUE:
            self.__eat()
            return ContinueStatement(kind=NodeType.CONTINUE_STATEMENT)
        elif current_token.type == TOKEN_TYPE.RETURN:
            return self.__parse_return_statement()

        return self.__parse_expression()

    def __parse_variable_declaration_statement(self: Self) -> Statement:
        self.__eat()
        identifier = self.__eat_exactly(TOKEN_TYPE.IDENTIFIER, None).value

        if self.__at().type == TOKEN_TYPE.EQUALS:
            self.__eat()

            return VariableDeclarationStatement(
                kind=NodeType.VARIABLE_DECLARATION_STATEMENT,
                identifier=identifier,
                value=self.__parse_expression_or_function_declaration(),
            )

        return VariableDeclarationStatement(
            kind=NodeType.VARIABLE_DECLARATION_STATEMENT,
            identifier=identifier,
            value=None,
        )

    def __parse_expression_or_function_declaration(
        self: Self,
    ) -> Expression | FunctionStatement:
        if self.__at().type == TOKEN_TYPE.FUNCTION:
            return self.__parse_function_declaration_statement()
        return self.__parse_expression()

    def __parse_print_statement(self: Self) -> Statement:
        self.__eat()

        return PrintStatement(
            kind=NodeType.PRINT_STATEMENT,
            value=self.__parse_expression(),
        )

    def __parse_if_statement(self: Self, end_needed: bool = True) -> Statement:
        if end_needed:
            self.__eat_exactly(TOKEN_TYPE.IF, None)

        pos_start = self.__previous().position.copy()
        condition = self.__parse_expression()
        self.__eat_exactly(TOKEN_TYPE.THEN, pos_start)

        pos_start = self.__previous().position.copy()
        then_branch = self.__parse_block_statement(
            [TOKEN_TYPE.ELSE, TOKEN_TYPE.ELSE_IF]
        )

        else_branch: Statement | None = None
        if self.__at().type == TOKEN_TYPE.ELSE:
            self.__eat()
            else_branch = self.__parse_block_statement()
        elif self.__at().type == TOKEN_TYPE.ELSE_IF:
            self.__eat()
            else_branch = self.__parse_if_statement(False)

        if end_needed:
            self.__eat_exactly(TOKEN_TYPE.END, pos_start)

        return IfStatement(
            kind=NodeType.IF_STATEMENT,
            condition=condition,
            then_branch=then_branch,
            else_branch=else_branch,
        )

    def __parse_while_statement(self: Self) -> Statement:
        self.__eat()
        pos_start = self.__previous().position.copy()

        condition = self.__parse_expression()
        self.__eat_exactly(TOKEN_TYPE.THEN, pos_start)

        body = self.__parse_block_statement()
        self.__eat_exactly(TOKEN_TYPE.END, pos_start)

        return WhileStatement(
            kind=NodeType.WHILE_STATEMENT,
            condition=condition,
            body=body,
        )

    def __parse_for_statement(self: Self) -> Statement:
        self.__eat()
        pos_start = self.__previous().position.copy()
        identifier = self.__eat_exactly(TOKEN_TYPE.IDENTIFIER, pos_start)

        self.__eat_exactly(TOKEN_TYPE.FROM, pos_start)
        init = self.__parse_expression()

        self.__eat_exactly(TOKEN_TYPE.UNTIL, pos_start)
        until = self.__parse_expression()
        if until.kind == NodeType.NUMERIC_LITERAL or until.kind == NodeType.IDENTIFIER:
            until = BinaryExpression(
                kind=NodeType.BINARY_EXPRESSION,
                operator="<=",
                left=Identifier(kind=NodeType.IDENTIFIER, identifier=identifier.value),
                right=until,
            )

        token = self.__at().type
        step: Expression = NumericLiteral(kind=NodeType.NUMERIC_LITERAL, value=1)
        if token == TOKEN_TYPE.STEP:
            self.__eat()
            step = self.__parse_expression()

        step = AssignmentExpression(
            kind=NodeType.ASSIGNMENT_EXPRESSION,
            assignment=Identifier(
                kind=NodeType.IDENTIFIER, identifier=identifier.value
            ),
            value=BinaryExpression(
                kind=NodeType.BINARY_EXPRESSION,
                operator="+",
                left=Identifier(kind=NodeType.IDENTIFIER, identifier=identifier.value),
                right=step,
            ),
        )

        self.__eat_exactly(TOKEN_TYPE.THEN, pos_start)
        body = self.__parse_block_statement()
        self.__eat_exactly(TOKEN_TYPE.END, pos_start)

        return ForStatement(
            kind=NodeType.FOR_STATEMENT,
            identifier=identifier.value,
            init=init,
            until=until,
            step=step,
            body=body,
        )

    def __parse_function_declaration_statement(self: Self) -> FunctionStatement:
        self.__eat()
        pos_start = self.__previous().position.copy()

        identifier = None
        if self.__at().type != TOKEN_TYPE.OPEN_PARENTHESIS:
            identifier = self.__eat_exactly(TOKEN_TYPE.IDENTIFIER, pos_start).value

        self.__eat_exactly(TOKEN_TYPE.OPEN_PARENTHESIS, pos_start)

        parameters: list[str] = []
        while self.__at().type != TOKEN_TYPE.CLOSE_PARENTHESIS:
            if len(parameters) >= 48:
                END_ERROR = "ne peut pas avoir plus de 48 paramètres"
                self.__raise_invalid_syntax(
                    pos_start,
                    self.__at().position,
                    f"La fonction '{identifier or 'anonyme'}' {END_ERROR}",
                )

            parameters.append(
                self.__eat_exactly(TOKEN_TYPE.IDENTIFIER, pos_start).value
            )

            token = self.__attempt([TOKEN_TYPE.COMMA, TOKEN_TYPE.CLOSE_PARENTHESIS])
            if token.type == TOKEN_TYPE.COMMA:
                self.__eat()

        self.__eat()
        body = self.__parse_block_statement()
        self.__eat_exactly(TOKEN_TYPE.END, pos_start)

        return FunctionStatement(
            kind=NodeType.FUNCTION_STATEMENT,
            identifier=identifier,
            parameters=parameters,
            body=body,
        )

    def __parse_block_statement(
        self: Self, with_condition: list[TOKEN_TYPE] | None = None
    ) -> BlockStatement:
        with_condition = with_condition if with_condition else []
        block = BlockStatement(
            kind=NodeType.BLOCK_STATEMENT,
            body=[],
        )

        while (
            self.__is_eof() is False
            and self.__at().type != TOKEN_TYPE.END
            and self.__at().type not in with_condition
        ):
            statement = self.__parse_statement()
            block.body.append(statement)

        return block

    def __parse_return_statement(self: Self) -> Statement:
        self.__eat()

        return ReturnStatement(
            kind=NodeType.RETURN_STATEMENT,
            value=self.__parse_expression(),
        )

    def __parse_expression(self: Self) -> Expression:
        return self.__parse_assignment_expression()

    def __parse_assignment_expression(self: Self) -> Expression:
        left = self.__parse_logical_expression()

        if self.__at().type != TOKEN_TYPE.EQUALS:
            return left

        self.__eat()
        return AssignmentExpression(
            kind=NodeType.ASSIGNMENT_EXPRESSION,
            assignment=left,
            value=self.__parse_assignment_expression(),
        )

    def __parse_logical_expression(self: Self) -> Expression:
        left = self.__parse_equality_expression()

        while self.__at().value in ["et", "ou"]:
            operator = self.__eat().value
            right = self.__parse_equality_expression()

            left = BinaryExpression(
                kind=NodeType.BINARY_EXPRESSION,
                operator=operator,
                left=left,
                right=right,
            )

        return left

    def __parse_equality_expression(self: Self) -> Expression:
        left = self.__parse_relational_expression()

        while self.__at().value in ["==", "!="]:
            operator = self.__eat().value
            right = self.__parse_relational_expression()

            left = BinaryExpression(
                kind=NodeType.BINARY_EXPRESSION,
                operator=operator,
                left=left,
                right=right,
            )

        return left

    def __parse_relational_expression(self: Self) -> Expression:
        left = self.__parse_additive_expression()

        while self.__at().value in ["<", ">", "<=", ">="]:
            operator = self.__eat().value
            right = self.__parse_additive_expression()

            left = BinaryExpression(
                kind=NodeType.BINARY_EXPRESSION,
                operator=operator,
                left=left,
                right=right,
            )

        return left

    def __parse_additive_expression(self: Self) -> Expression:
        left = self.__parse_multiplicative_expression()

        while self.__at().value in ["+", "-"]:
            operator = self.__eat().value
            right = self.__parse_multiplicative_expression()

            left = BinaryExpression(
                kind=NodeType.BINARY_EXPRESSION,
                operator=operator,
                left=left,
                right=right,
            )

        return left

    def __parse_multiplicative_expression(self: Self) -> Expression:
        left = self.__parse_unary_expression()

        while self.__at().value in ["*", "/", "%"]:
            operator = self.__eat().value
            right = self.__parse_unary_expression()

            left = BinaryExpression(
                kind=NodeType.BINARY_EXPRESSION,
                operator=operator,
                left=left,
                right=right,
            )

        return left

    def __parse_unary_expression(self: Self) -> Expression:
        if self.__at().type != TOKEN_TYPE.UNARY_OPERATOR:
            return self.__parse_array_access_expression()

        operator = self.__eat().value
        value = self.__parse_unary_expression()

        return UnaryExpression(
            kind=NodeType.UNARY_EXPRESSION,
            operator=operator,
            value=value,
        )

    def __parse_array_access_expression(self: Self) -> Expression:
        expression = self.__parse_array_expression()

        while self.__at().type == TOKEN_TYPE.OPEN_BRACKET:
            self.__eat()

            if self.__at().type == TOKEN_TYPE.CLOSE_BRACKET:
                self.__eat()
                return MemberExpression(
                    kind=NodeType.MEMBER_EXPRESSION,
                    object=expression,
                    property=None,
                )

            index = self.__parse_expression()
            self.__eat_exactly(TOKEN_TYPE.CLOSE_BRACKET, self.__previous().position)

            expression = MemberExpression(
                kind=NodeType.MEMBER_EXPRESSION,
                object=expression,
                property=index,
            )

        return expression

    def __parse_array_expression(self: Self) -> Expression:
        if self.__at().type != TOKEN_TYPE.OPEN_BRACKET:
            return self.__parse_call_expression()

        self.__eat()

        elements = []
        while self.__at().type != TOKEN_TYPE.CLOSE_BRACKET:
            elements.append(self.__parse_expression())

            token = self.__attempt([TOKEN_TYPE.COMMA, TOKEN_TYPE.CLOSE_BRACKET])
            if token.type == TOKEN_TYPE.COMMA:
                self.__eat()

        self.__eat()
        return ArrayExpression(
            kind=NodeType.ARRAY_EXPRESSION,
            elements=elements,
        )

    def __parse_call_expression(self: Self) -> Expression:
        expression = self.__parse_primary_expression()

        while self.__at().type == TOKEN_TYPE.OPEN_PARENTHESIS:
            self.__eat()
            arguments = self.__parse_argument_list()
            self.__eat()

            expression = CallExpression(
                kind=NodeType.CALL_EXPRESSION,
                callee=expression,
                arguments=arguments,
            )

        return expression

    def __parse_argument_list(
        self: Self,
    ) -> list[Expression | FunctionStatement]:
        ATTEMPTED = [TOKEN_TYPE.COMMA, TOKEN_TYPE.CLOSE_PARENTHESIS]

        arguments = []
        while self.__at().type != TOKEN_TYPE.CLOSE_PARENTHESIS:
            new_argument = self.__parse_expression_or_function_declaration()
            arguments.append(new_argument)

            token = self.__attempt(ATTEMPTED)
            if token.type == TOKEN_TYPE.COMMA:
                self.__eat()

        return arguments

    def __parse_primary_expression(self: Self) -> Expression:
        token_type = self.__at().type

        if token_type == TOKEN_TYPE.IDENTIFIER:
            return Identifier(
                kind=NodeType.IDENTIFIER,
                identifier=self.__eat().value,
            )
        elif token_type == TOKEN_TYPE.NULL:
            self.__eat()
            return NullLiteral(
                kind=NodeType.NULL_LITERAL,
                value=None,
            )
        elif token_type == TOKEN_TYPE.BOOLEAN:
            return BooleanLiteral(
                kind=NodeType.BOOLEAN_LITERAL,
                value=self.__eat().value == "vrai",
            )
        elif token_type == TOKEN_TYPE.NUMBER:
            return NumericLiteral(
                kind=NodeType.NUMERIC_LITERAL,
                value=float(self.__eat().value),
            )
        elif token_type == TOKEN_TYPE.STRING:
            return StringLiteral(
                kind=NodeType.STRING_LITERAL,
                value=self.__eat().value,
            )
        elif token_type == TOKEN_TYPE.OPEN_PARENTHESIS:
            self.__eat()
            expression = self.__parse_expression()
            self.__eat_exactly(TOKEN_TYPE.CLOSE_PARENTHESIS, self.__previous().position)

            return expression

        pos_start = self.__previous().position
        pos_end = self.__at().position

        self.__raise_invalid_syntax(
            pos_start,
            pos_end,
            f"'{self.__at().value}' non attendu, expression attendue",
        )

        # Unreachable
        return NullLiteral(kind=NodeType.NULL_LITERAL, value=None)

    def __is_eof(self: Self) -> bool:
        return self.__at().type == TOKEN_TYPE.EOF

    def __at(self: Self) -> Token:
        return self.tokens[0] if self.tokens else create_eof_token()

    def __previous(self: Self) -> Token:
        return self.previous_token if self.previous_token else create_eof_token()

    def __eat(self: Self) -> Token:
        self.previous_token = self.__at()
        return self.tokens.pop(0) if self.tokens else create_eof_token()

    def __eat_exactly(
        self: Self, type: TOKEN_TYPE, pos_start: Position | None
    ) -> Token:
        token = self.__eat()

        if token.type != type:
            pos_start = pos_start if pos_start else token.position
            pos_end = token.position
            tokens_needed = find_keyword_from_token(type)
            types_str = " ou ".join([f"'{type}'" for type in tokens_needed])

            if token.type == TOKEN_TYPE.EOF:
                pos_start_eof = pos_start.copy()
                pos_start_eof.content = "<EOF>"

                self.__raise_invalid_syntax(
                    pos_end,
                    pos_start_eof,
                    f"Fin de fichier atteinte, '{types_str}' attendu",
                )

            self.__raise_invalid_syntax(
                pos_start,
                pos_end,
                f"'{token.value}' trouvé, '{types_str}' attendu",
            )

        return token

    def __attempt(self: Self, types: list[TOKEN_TYPE]) -> Token:
        token = self.__at()

        if token.type in types:
            return token

        types_str = " ou ".join([f"'{type.name}'" for type in types])
        self.__raise_invalid_syntax(
            token.position,
            token.position,
            f"'{token.value}' trouvé, '{types_str}' attendu",
        )
        return create_eof_token()  # Unreachable

    def __raise_invalid_syntax(
        self: Self, pos_start: Position, pos_end: Position, message: str
    ) -> None:
        raise InvalidSyntaxException(pos_start, pos_end, message, self.code)
