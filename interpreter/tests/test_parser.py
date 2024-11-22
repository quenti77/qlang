import pytest  # type: ignore

from src.enums.operators import OPERATORS
from src.errors.invalid_syntax_exception import InvalidSyntaxException
from src.lexer import Lexer
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
from src.nodes.merged import BlockStatement
from src.nodes.merged import FunctionStatement
from src.nodes.node_type import NodeType
from src.nodes.statement import ForStatement
from src.nodes.statement import IfStatement
from src.nodes.statement import PrintStatement
from src.nodes.statement import Program
from src.nodes.statement import VariableDeclarationStatement
from src.nodes.statement import WhileStatement
from src.parser import Parser


def init() -> tuple[Lexer, Parser]:
    return (Lexer(), Parser())


def make_ast_from_input(input: str, lexer: Lexer, parser: Parser) -> Program:
    lexer.tokenize(input)
    parser.setTokens(lexer.get_tokens(), input)
    return parser.make_ast()


def test_empty_program() -> None:
    lexer, parser = init()
    assert make_ast_from_input("", lexer, parser) == Program(
        kind=NodeType.PROGAM, body=[]
    )


def test_make_ast_identifier() -> None:
    lexer, parser = init()
    identifier = Identifier(kind=NodeType.IDENTIFIER, identifier="foo")

    assert make_ast_from_input("foo", lexer, parser) == Program(
        kind=NodeType.PROGAM,
        body=[identifier],
    )


def test_make_ast_numeric_literal() -> None:
    lexer, parser = init()
    numeric_literal = NumericLiteral(kind=NodeType.NUMERIC_LITERAL, value=1.2)

    assert make_ast_from_input("1.2", lexer, parser) == Program(
        kind=NodeType.PROGAM,
        body=[numeric_literal],
    )


def test_make_ast_null() -> None:
    lexer, parser = init()
    assert make_ast_from_input("rien", lexer, parser) == Program(
        kind=NodeType.PROGAM,
        body=[NullLiteral(kind=NodeType.NULL_LITERAL, value=None)],
    )


@pytest.mark.parametrize("operator", OPERATORS)
def test_tokenize_an_operator(operator) -> None:
    lexer, parser = init()
    left_expr = NumericLiteral(kind=NodeType.NUMERIC_LITERAL, value=1)
    right_expr = NumericLiteral(kind=NodeType.NUMERIC_LITERAL, value=2)
    binary_expr = BinaryExpression(
        kind=NodeType.BINARY_EXPRESSION,
        operator=operator,
        left=left_expr,
        right=right_expr,
    )

    assert make_ast_from_input(f"1 {operator} 2", lexer, parser) == Program(
        kind=NodeType.PROGAM,
        body=[binary_expr],
    )


def test_make_ast_priority_expression() -> None:
    lexer, parser = init()
    left_expr = NumericLiteral(kind=NodeType.NUMERIC_LITERAL, value=5)
    right_expr = BinaryExpression(
        kind=NodeType.BINARY_EXPRESSION,
        operator="+",
        left=NumericLiteral(kind=NodeType.NUMERIC_LITERAL, value=2),
        right=NumericLiteral(kind=NodeType.NUMERIC_LITERAL, value=3),
    )
    binary_expr = BinaryExpression(
        kind=NodeType.BINARY_EXPRESSION,
        operator="*",
        left=left_expr,
        right=right_expr,
    )

    assert make_ast_from_input("5 * (2 + 3)", lexer, parser) == Program(
        kind=NodeType.PROGAM,
        body=[binary_expr],
    )


@pytest.mark.parametrize("value", ["vrai", "faux"])
def test_make_ast_boolean_literal(value) -> None:
    lexer, parser = init()
    assert make_ast_from_input(value, lexer, parser) == Program(
        kind=NodeType.PROGAM,
        body=[BooleanLiteral(kind=NodeType.BOOLEAN_LITERAL, value=value == "vrai")],
    )


def test_make_ast_string_literal() -> None:
    lexer, parser = init()
    assert make_ast_from_input('"Hello, World!"', lexer, parser) == Program(
        kind=NodeType.PROGAM,
        body=[StringLiteral(kind=NodeType.STRING_LITERAL, value="Hello, World!")],
    )


def test_make_ast_variable_declaration() -> None:
    lexer, parser = init()
    variable_declaration = VariableDeclarationStatement(
        kind=NodeType.VARIABLE_DECLARATION_STATEMENT,
        identifier="abc",
        value=NumericLiteral(kind=NodeType.NUMERIC_LITERAL, value=3.14),
    )

    assert make_ast_from_input("dec abc = 3.14", lexer, parser) == Program(
        kind=NodeType.PROGAM,
        body=[variable_declaration],
    )


def test_make_ast_variable_declaration_with_new_line_expression() -> None:
    lexer, parser = init()
    binary_expr = BinaryExpression(
        kind=NodeType.BINARY_EXPRESSION,
        operator="+",
        left=NumericLiteral(kind=NodeType.NUMERIC_LITERAL, value=39),
        right=NumericLiteral(kind=NodeType.NUMERIC_LITERAL, value=3),
    )
    variable_declaration = VariableDeclarationStatement(
        kind=NodeType.VARIABLE_DECLARATION_STATEMENT,
        identifier="abc",
        value=binary_expr,
    )

    input = ["dec abc =", "39 + 3"]
    assert make_ast_from_input("\n".join(input), lexer, parser) == Program(
        kind=NodeType.PROGAM,
        body=[variable_declaration],
    )


def test_make_ast_variable_declaration_without_value() -> None:
    lexer, parser = init()
    variable_declaration = VariableDeclarationStatement(
        kind=NodeType.VARIABLE_DECLARATION_STATEMENT,
        identifier="abc",
        value=None,
    )

    assert make_ast_from_input("dec abc", lexer, parser) == Program(
        kind=NodeType.PROGAM,
        body=[variable_declaration],
    )


def test_make_ast_assignment() -> None:
    lexer, parser = init()
    variable_declaration = VariableDeclarationStatement(
        kind=NodeType.VARIABLE_DECLARATION_STATEMENT,
        identifier="abc",
        value=NumericLiteral(kind=NodeType.NUMERIC_LITERAL, value=3.14),
    )
    assignment = AssignmentExpression(
        kind=NodeType.ASSIGNMENT_EXPRESSION,
        assignment=Identifier(kind=NodeType.IDENTIFIER, identifier="abc"),
        value=NumericLiteral(kind=NodeType.NUMERIC_LITERAL, value=42),
    )

    input = ["dec abc = 3.14", "abc = 42"]
    assert make_ast_from_input("\n".join(input), lexer, parser) == Program(
        kind=NodeType.PROGAM,
        body=[variable_declaration, assignment],
    )


def test_make_ast_multiple_assignment() -> None:
    lexer, parser = init()
    variable_declaration = VariableDeclarationStatement(
        kind=NodeType.VARIABLE_DECLARATION_STATEMENT,
        identifier="abc",
        value=NumericLiteral(kind=NodeType.NUMERIC_LITERAL, value=3.14),
    )
    variable_declaration_2 = VariableDeclarationStatement(
        kind=NodeType.VARIABLE_DECLARATION_STATEMENT,
        identifier="def",
        value=Identifier(kind=NodeType.IDENTIFIER, identifier="abc"),
    )
    assignment = AssignmentExpression(
        kind=NodeType.ASSIGNMENT_EXPRESSION,
        assignment=Identifier(kind=NodeType.IDENTIFIER, identifier="abc"),
        value=AssignmentExpression(
            kind=NodeType.ASSIGNMENT_EXPRESSION,
            assignment=Identifier(kind=NodeType.IDENTIFIER, identifier="def"),
            value=NumericLiteral(kind=NodeType.NUMERIC_LITERAL, value=42),
        ),
    )

    input = [
        "dec abc = 3.14",
        "dec def = abc",
        "abc = def = 42",
    ]
    assert make_ast_from_input("\n".join(input), lexer, parser) == Program(
        kind=NodeType.PROGAM,
        body=[variable_declaration, variable_declaration_2, assignment],
    )


def test_make_ast_print_statement() -> None:
    lexer, parser = init()
    print_statement = PrintStatement(
        kind=NodeType.PRINT_STATEMENT,
        value=NumericLiteral(kind=NodeType.NUMERIC_LITERAL, value=42),
    )

    assert make_ast_from_input("ecrire 42", lexer, parser) == Program(
        kind=NodeType.PROGAM,
        body=[print_statement],
    )


def test_make_ast_if_statement_define_block() -> None:
    lexer, parser = init()
    if_block = BlockStatement(
        kind=NodeType.BLOCK_STATEMENT,
        body=[
            PrintStatement(
                kind=NodeType.PRINT_STATEMENT,
                value=NumericLiteral(kind=NodeType.NUMERIC_LITERAL, value=42),
            )
        ],
    )
    if_statement = IfStatement(
        kind=NodeType.IF_STATEMENT,
        condition=NumericLiteral(kind=NodeType.NUMERIC_LITERAL, value=42),
        then_branch=if_block,
        else_branch=None,
    )

    input = [
        "si 42 alors",
        "  ecrire 42",
        "fin",
    ]
    assert make_ast_from_input("\n".join(input), lexer, parser) == Program(
        kind=NodeType.PROGAM,
        body=[if_statement],
    )


def test_make_ast_if_else_define_blocks() -> None:
    lexer, parser = init()
    if_block = BlockStatement(
        kind=NodeType.BLOCK_STATEMENT,
        body=[
            PrintStatement(
                kind=NodeType.PRINT_STATEMENT,
                value=NumericLiteral(kind=NodeType.NUMERIC_LITERAL, value=42),
            )
        ],
    )
    else_block = BlockStatement(
        kind=NodeType.BLOCK_STATEMENT,
        body=[
            PrintStatement(
                kind=NodeType.PRINT_STATEMENT,
                value=NumericLiteral(kind=NodeType.NUMERIC_LITERAL, value=24),
            )
        ],
    )
    if_statement = IfStatement(
        kind=NodeType.IF_STATEMENT,
        condition=NumericLiteral(kind=NodeType.NUMERIC_LITERAL, value=42),
        then_branch=if_block,
        else_branch=else_block,
    )

    input = [
        "si 42 alors",
        "  ecrire 42",
        "sinon",
        "  ecrire 24",
        "fin",
    ]
    assert make_ast_from_input("\n".join(input), lexer, parser) == Program(
        kind=NodeType.PROGAM,
        body=[if_statement],
    )


def test_make_ast_multiple_elseif_blocks() -> None:
    lexer, parser = init()
    if_block = BlockStatement(
        kind=NodeType.BLOCK_STATEMENT,
        body=[
            PrintStatement(
                kind=NodeType.PRINT_STATEMENT,
                value=NumericLiteral(kind=NodeType.NUMERIC_LITERAL, value=42),
            )
        ],
    )
    second_if_block = BlockStatement(
        kind=NodeType.BLOCK_STATEMENT,
        body=[
            PrintStatement(
                kind=NodeType.PRINT_STATEMENT,
                value=NumericLiteral(kind=NodeType.NUMERIC_LITERAL, value=2),
            )
        ],
    )
    third_if_block = BlockStatement(
        kind=NodeType.BLOCK_STATEMENT,
        body=[
            PrintStatement(
                kind=NodeType.PRINT_STATEMENT,
                value=NumericLiteral(kind=NodeType.NUMERIC_LITERAL, value=3),
            )
        ],
    )
    else_block = BlockStatement(
        kind=NodeType.BLOCK_STATEMENT,
        body=[
            PrintStatement(
                kind=NodeType.PRINT_STATEMENT,
                value=NumericLiteral(kind=NodeType.NUMERIC_LITERAL, value=24),
            )
        ],
    )
    if_statement = IfStatement(
        kind=NodeType.IF_STATEMENT,
        condition=NumericLiteral(kind=NodeType.NUMERIC_LITERAL, value=42),
        then_branch=if_block,
        else_branch=IfStatement(
            kind=NodeType.IF_STATEMENT,
            condition=NumericLiteral(kind=NodeType.NUMERIC_LITERAL, value=2),
            then_branch=second_if_block,
            else_branch=IfStatement(
                kind=NodeType.IF_STATEMENT,
                condition=NumericLiteral(kind=NodeType.NUMERIC_LITERAL, value=3),
                then_branch=third_if_block,
                else_branch=else_block,
            ),
        ),
    )

    input = [
        "si 42 alors",
        "  ecrire 42",
        "sinonsi 2 alors",
        "  ecrire 2",
        "sinonsi 3 alors",
        "  ecrire 3",
        "sinon",
        "  ecrire 24",
        "fin",
    ]
    assert make_ast_from_input("\n".join(input), lexer, parser) == Program(
        kind=NodeType.PROGAM,
        body=[if_statement],
    )


def test_make_ast_if_include_in_if() -> None:
    lexer, parser = init()
    if_block = BlockStatement(
        kind=NodeType.BLOCK_STATEMENT,
        body=[
            PrintStatement(
                kind=NodeType.PRINT_STATEMENT,
                value=NumericLiteral(kind=NodeType.NUMERIC_LITERAL, value=42),
            )
        ],
    )
    if_inner_block = BlockStatement(
        kind=NodeType.BLOCK_STATEMENT,
        body=[
            PrintStatement(
                kind=NodeType.PRINT_STATEMENT,
                value=NumericLiteral(kind=NodeType.NUMERIC_LITERAL, value=24),
            )
        ],
    )
    else_block = BlockStatement(
        kind=NodeType.BLOCK_STATEMENT,
        body=[
            IfStatement(
                kind=NodeType.IF_STATEMENT,
                condition=NumericLiteral(kind=NodeType.NUMERIC_LITERAL, value=24),
                then_branch=if_inner_block,
                else_branch=None,
            )
        ],
    )
    if_statement = IfStatement(
        kind=NodeType.IF_STATEMENT,
        condition=NumericLiteral(kind=NodeType.NUMERIC_LITERAL, value=42),
        then_branch=if_block,
        else_branch=else_block,
    )

    input = [
        "si 42 alors",
        "  ecrire 42",
        "sinon",
        "  si 24 alors",
        "    ecrire 24",
        "  fin",
        "fin",
    ]
    assert make_ast_from_input("\n".join(input), lexer, parser) == Program(
        kind=NodeType.PROGAM,
        body=[if_statement],
    )


def test_make_ast_multiple_statement_in_if_block() -> None:
    lexer, parser = init()
    if_block = BlockStatement(
        kind=NodeType.BLOCK_STATEMENT,
        body=[
            PrintStatement(
                kind=NodeType.PRINT_STATEMENT,
                value=NumericLiteral(kind=NodeType.NUMERIC_LITERAL, value=42),
            ),
            PrintStatement(
                kind=NodeType.PRINT_STATEMENT,
                value=NumericLiteral(kind=NodeType.NUMERIC_LITERAL, value=24),
            ),
        ],
    )
    if_statement = IfStatement(
        kind=NodeType.IF_STATEMENT,
        condition=NumericLiteral(kind=NodeType.NUMERIC_LITERAL, value=42),
        then_branch=if_block,
        else_branch=None,
    )

    input = [
        "si 42 alors",
        "  ecrire 42",
        "  ecrire 24",
        "fin",
    ]
    assert make_ast_from_input("\n".join(input), lexer, parser) == Program(
        kind=NodeType.PROGAM,
        body=[if_statement],
    )


def test_make_ast_if_condition_with_and_or() -> None:
    lexer, parser = init()
    if_block = BlockStatement(
        kind=NodeType.BLOCK_STATEMENT,
        body=[
            PrintStatement(
                kind=NodeType.PRINT_STATEMENT,
                value=NumericLiteral(kind=NodeType.NUMERIC_LITERAL, value=42),
            )
        ],
    )
    if_statement = IfStatement(
        kind=NodeType.IF_STATEMENT,
        condition=BinaryExpression(
            kind=NodeType.BINARY_EXPRESSION,
            operator="ou",
            left=BinaryExpression(
                kind=NodeType.BINARY_EXPRESSION,
                operator="et",
                left=BinaryExpression(
                    kind=NodeType.BINARY_EXPRESSION,
                    operator="<",
                    left=Identifier(kind=NodeType.IDENTIFIER, identifier="a"),
                    right=Identifier(kind=NodeType.IDENTIFIER, identifier="b"),
                ),
                right=BinaryExpression(
                    kind=NodeType.BINARY_EXPRESSION,
                    operator="<",
                    left=Identifier(kind=NodeType.IDENTIFIER, identifier="a"),
                    right=Identifier(kind=NodeType.IDENTIFIER, identifier="c"),
                ),
            ),
            right=BinaryExpression(
                kind=NodeType.BINARY_EXPRESSION,
                operator="<",
                left=Identifier(kind=NodeType.IDENTIFIER, identifier="b"),
                right=Identifier(kind=NodeType.IDENTIFIER, identifier="c"),
            ),
        ),
        then_branch=if_block,
        else_branch=None,
    )

    input = [
        "si a < b et a < c ou b < c alors",
        "  ecrire 42",
        "fin",
    ]
    assert make_ast_from_input("\n".join(input), lexer, parser) == Program(
        kind=NodeType.PROGAM,
        body=[if_statement],
    )


def test_make_ast_if_condition_with_and_or_in_parenthesis() -> None:
    lexer, parser = init()
    if_block = BlockStatement(
        kind=NodeType.BLOCK_STATEMENT,
        body=[
            PrintStatement(
                kind=NodeType.PRINT_STATEMENT,
                value=NumericLiteral(kind=NodeType.NUMERIC_LITERAL, value=42),
            )
        ],
    )
    if_statement = IfStatement(
        kind=NodeType.IF_STATEMENT,
        condition=BinaryExpression(
            kind=NodeType.BINARY_EXPRESSION,
            operator="et",
            left=BinaryExpression(
                kind=NodeType.BINARY_EXPRESSION,
                operator="<",
                left=Identifier(kind=NodeType.IDENTIFIER, identifier="a"),
                right=Identifier(kind=NodeType.IDENTIFIER, identifier="b"),
            ),
            right=BinaryExpression(
                kind=NodeType.BINARY_EXPRESSION,
                operator="ou",
                left=BinaryExpression(
                    kind=NodeType.BINARY_EXPRESSION,
                    operator="<",
                    left=Identifier(kind=NodeType.IDENTIFIER, identifier="a"),
                    right=Identifier(kind=NodeType.IDENTIFIER, identifier="c"),
                ),
                right=BinaryExpression(
                    kind=NodeType.BINARY_EXPRESSION,
                    operator="<",
                    left=Identifier(kind=NodeType.IDENTIFIER, identifier="b"),
                    right=Identifier(kind=NodeType.IDENTIFIER, identifier="c"),
                ),
            ),
        ),
        then_branch=if_block,
        else_branch=None,
    )

    input = [
        "si a < b et (a < c ou b < c) alors",
        "  ecrire 42",
        "fin",
    ]
    assert make_ast_from_input("\n".join(input), lexer, parser) == Program(
        kind=NodeType.PROGAM,
        body=[if_statement],
    )


def test_make_ast_while_statement() -> None:
    lexer, parser = init()
    while_statement = WhileStatement(
        kind=NodeType.WHILE_STATEMENT,
        condition=BooleanLiteral(kind=NodeType.BOOLEAN_LITERAL, value=True),
        body=BlockStatement(
            kind=NodeType.BLOCK_STATEMENT,
            body=[
                PrintStatement(
                    kind=NodeType.PRINT_STATEMENT,
                    value=NumericLiteral(kind=NodeType.NUMERIC_LITERAL, value=42),
                )
            ],
        ),
    )

    input = [
        "tantque vrai alors",
        "  ecrire 42",
        "fin",
    ]
    assert make_ast_from_input("\n".join(input), lexer, parser) == Program(
        kind=NodeType.PROGAM,
        body=[while_statement],
    )


def test_make_ast_for_statement() -> None:
    lexer, parser = init()
    for_body = BlockStatement(
        kind=NodeType.BLOCK_STATEMENT,
        body=[
            PrintStatement(
                kind=NodeType.PRINT_STATEMENT,
                value=Identifier(kind=NodeType.IDENTIFIER, identifier="abc"),
            )
        ],
    )
    for_statement = ForStatement(
        kind=NodeType.FOR_STATEMENT,
        identifier="abc",
        init=NumericLiteral(kind=NodeType.NUMERIC_LITERAL, value=1.0),
        until=BinaryExpression(
            kind=NodeType.BINARY_EXPRESSION,
            operator="<=",
            left=Identifier(kind=NodeType.IDENTIFIER, identifier="abc"),
            right=NumericLiteral(kind=NodeType.NUMERIC_LITERAL, value=10.0),
        ),
        step=AssignmentExpression(
            kind=NodeType.ASSIGNMENT_EXPRESSION,
            assignment=Identifier(kind=NodeType.IDENTIFIER, identifier="abc"),
            value=BinaryExpression(
                kind=NodeType.BINARY_EXPRESSION,
                operator="+",
                left=Identifier(kind=NodeType.IDENTIFIER, identifier="abc"),
                right=NumericLiteral(kind=NodeType.NUMERIC_LITERAL, value=2.0),
            ),
        ),
        body=for_body,
    )

    input = [
        "pour abc de 1 jusque 10 evol 2 alors",
        "  ecrire abc",
        "fin",
    ]
    assert make_ast_from_input("\n".join(input), lexer, parser) == Program(
        kind=NodeType.PROGAM,
        body=[for_statement],
    )


def test_make_ast_for_statement_without_evol() -> None:
    lexer, parser = init()
    for_body = BlockStatement(
        kind=NodeType.BLOCK_STATEMENT,
        body=[
            PrintStatement(
                kind=NodeType.PRINT_STATEMENT,
                value=Identifier(kind=NodeType.IDENTIFIER, identifier="abc"),
            )
        ],
    )
    for_statement = ForStatement(
        kind=NodeType.FOR_STATEMENT,
        identifier="abc",
        init=NumericLiteral(kind=NodeType.NUMERIC_LITERAL, value=1.0),
        until=BinaryExpression(
            kind=NodeType.BINARY_EXPRESSION,
            operator="<=",
            left=Identifier(kind=NodeType.IDENTIFIER, identifier="abc"),
            right=NumericLiteral(kind=NodeType.NUMERIC_LITERAL, value=10.0),
        ),
        step=AssignmentExpression(
            kind=NodeType.ASSIGNMENT_EXPRESSION,
            assignment=Identifier(kind=NodeType.IDENTIFIER, identifier="abc"),
            value=BinaryExpression(
                kind=NodeType.BINARY_EXPRESSION,
                operator="+",
                left=Identifier(kind=NodeType.IDENTIFIER, identifier="abc"),
                right=NumericLiteral(kind=NodeType.NUMERIC_LITERAL, value=1.0),
            ),
        ),
        body=for_body,
    )

    input = [
        "pour abc de 1 jusque 10 alors",
        "  ecrire abc",
        "fin",
    ]
    assert make_ast_from_input("\n".join(input), lexer, parser) == Program(
        kind=NodeType.PROGAM,
        body=[for_statement],
    )


def test_make_ast_simple_array() -> None:
    lexer, parser = init()
    array_expr = ArrayExpression(
        kind=NodeType.ARRAY_EXPRESSION,
        elements=[
            NumericLiteral(kind=NodeType.NUMERIC_LITERAL, value=1),
            NumericLiteral(kind=NodeType.NUMERIC_LITERAL, value=2),
            NumericLiteral(kind=NodeType.NUMERIC_LITERAL, value=3),
        ],
    )

    assert make_ast_from_input("[1, 2, 3]", lexer, parser) == Program(
        kind=NodeType.PROGAM,
        body=[array_expr],
    )


def test_make_ast_nested_array() -> None:
    lexer, parser = init()
    array_expr = ArrayExpression(
        kind=NodeType.ARRAY_EXPRESSION,
        elements=[
            NumericLiteral(kind=NodeType.NUMERIC_LITERAL, value=1),
            ArrayExpression(
                kind=NodeType.ARRAY_EXPRESSION,
                elements=[
                    NumericLiteral(kind=NodeType.NUMERIC_LITERAL, value=2),
                    NumericLiteral(kind=NodeType.NUMERIC_LITERAL, value=3),
                ],
            ),
            NumericLiteral(kind=NodeType.NUMERIC_LITERAL, value=4),
        ],
    )

    assert make_ast_from_input("[1, [2, 3], 4,]", lexer, parser) == Program(
        kind=NodeType.PROGAM,
        body=[array_expr],
    )


@pytest.mark.parametrize("input", ["[1, 2, 3", "[1, 2 3]", "[1, 2, 3[]"])
def test_make_ast_invalid_array(input) -> None:
    lexer, parser = init()
    with pytest.raises(InvalidSyntaxException):
        make_ast_from_input(input, lexer, parser)


def test_make_ast_array_access() -> None:
    lexer, parser = init()
    variable_declaration = VariableDeclarationStatement(
        kind=NodeType.VARIABLE_DECLARATION_STATEMENT,
        identifier="abc",
        value=ArrayExpression(
            kind=NodeType.ARRAY_EXPRESSION,
            elements=[
                NumericLiteral(kind=NodeType.NUMERIC_LITERAL, value=1),
                NumericLiteral(kind=NodeType.NUMERIC_LITERAL, value=2),
                NumericLiteral(kind=NodeType.NUMERIC_LITERAL, value=3),
            ],
        ),
    )
    member_expr = MemberExpression(
        kind=NodeType.MEMBER_EXPRESSION,
        object=Identifier(kind=NodeType.IDENTIFIER, identifier="abc"),
        property=NumericLiteral(kind=NodeType.NUMERIC_LITERAL, value=1),
    )

    input = [
        "dec abc = [1, 2, 3]",
        "ecrire abc[1]",
    ]
    assert make_ast_from_input("\n".join(input), lexer, parser) == Program(
        kind=NodeType.PROGAM,
        body=[
            variable_declaration,
            PrintStatement(kind=NodeType.PRINT_STATEMENT, value=member_expr),
        ],
    )


def test_make_ast_array_access_from_declaration() -> None:
    lexer, parser = init()
    array_expr = ArrayExpression(
        kind=NodeType.ARRAY_EXPRESSION,
        elements=[
            NumericLiteral(kind=NodeType.NUMERIC_LITERAL, value=1),
            NumericLiteral(kind=NodeType.NUMERIC_LITERAL, value=2),
            NumericLiteral(kind=NodeType.NUMERIC_LITERAL, value=3),
        ],
    )
    member_expr = MemberExpression(
        kind=NodeType.MEMBER_EXPRESSION,
        object=array_expr,
        property=NumericLiteral(kind=NodeType.NUMERIC_LITERAL, value=1),
    )

    assert make_ast_from_input("[1, 2, 3][1]", lexer, parser) == Program(
        kind=NodeType.PROGAM,
        body=[member_expr],
    )


def test_make_ast_array_access_with_variable() -> None:
    lexer, parser = init()
    array_expr = ArrayExpression(
        kind=NodeType.ARRAY_EXPRESSION,
        elements=[
            NumericLiteral(kind=NodeType.NUMERIC_LITERAL, value=1),
            ArrayExpression(
                kind=NodeType.ARRAY_EXPRESSION,
                elements=[
                    NumericLiteral(kind=NodeType.NUMERIC_LITERAL, value=2),
                    NumericLiteral(kind=NodeType.NUMERIC_LITERAL, value=3),
                ],
            ),
            NumericLiteral(kind=NodeType.NUMERIC_LITERAL, value=4),
        ],
    )
    tab_declaration = VariableDeclarationStatement(
        kind=NodeType.VARIABLE_DECLARATION_STATEMENT,
        identifier="tab",
        value=array_expr,
    )
    index_declaration = VariableDeclarationStatement(
        kind=NodeType.VARIABLE_DECLARATION_STATEMENT,
        identifier="index",
        value=NumericLiteral(kind=NodeType.NUMERIC_LITERAL, value=1),
    )
    member_expr = MemberExpression(
        kind=NodeType.MEMBER_EXPRESSION,
        object=MemberExpression(
            kind=NodeType.MEMBER_EXPRESSION,
            object=Identifier(kind=NodeType.IDENTIFIER, identifier="tab"),
            property=Identifier(kind=NodeType.IDENTIFIER, identifier="index"),
        ),
        property=Identifier(kind=NodeType.IDENTIFIER, identifier="index"),
    )
    print_statement = PrintStatement(
        kind=NodeType.PRINT_STATEMENT,
        value=member_expr,
    )

    input = [
        "dec tab = [1, [2, 3], 4]",
        "dec index = 1",
        "ecrire tab[index][index]",
    ]
    assert make_ast_from_input("\n".join(input), lexer, parser) == Program(
        kind=NodeType.PROGAM,
        body=[tab_declaration, index_declaration, print_statement],
    )


def test_make_ast_push_item_in_array() -> None:
    lexer, parser = init()
    tab_declaration = VariableDeclarationStatement(
        kind=NodeType.VARIABLE_DECLARATION_STATEMENT,
        identifier="tab",
        value=ArrayExpression(
            kind=NodeType.ARRAY_EXPRESSION,
            elements=[],
        ),
    )
    tab_push = AssignmentExpression(
        kind=NodeType.ASSIGNMENT_EXPRESSION,
        assignment=MemberExpression(
            kind=NodeType.MEMBER_EXPRESSION,
            object=Identifier(kind=NodeType.IDENTIFIER, identifier="tab"),
            property=None,
        ),
        value=NumericLiteral(kind=NodeType.NUMERIC_LITERAL, value=1),
    )

    input = [
        "dec tab = []",
        "tab[] = 1",
    ]
    assert make_ast_from_input("\n".join(input), lexer, parser) == Program(
        kind=NodeType.PROGAM,
        body=[tab_declaration, tab_push],
    )


def test_make_ast_push_item_in_sub_array() -> None:
    lexer, parser = init()
    tab_declaration = VariableDeclarationStatement(
        kind=NodeType.VARIABLE_DECLARATION_STATEMENT,
        identifier="tab",
        value=ArrayExpression(
            kind=NodeType.ARRAY_EXPRESSION,
            elements=[
                ArrayExpression(
                    kind=NodeType.ARRAY_EXPRESSION,
                    elements=[],
                ),
            ],
        ),
    )
    tab_push = AssignmentExpression(
        kind=NodeType.ASSIGNMENT_EXPRESSION,
        assignment=MemberExpression(
            kind=NodeType.MEMBER_EXPRESSION,
            object=MemberExpression(
                kind=NodeType.MEMBER_EXPRESSION,
                object=Identifier(kind=NodeType.IDENTIFIER, identifier="tab"),
                property=NumericLiteral(kind=NodeType.NUMERIC_LITERAL, value=0),
            ),
            property=None,
        ),
        value=NumericLiteral(kind=NodeType.NUMERIC_LITERAL, value=1),
    )

    input = [
        "dec tab = [[]]",
        "tab[0][] = 1",
    ]
    assert make_ast_from_input("\n".join(input), lexer, parser) == Program(
        kind=NodeType.PROGAM,
        body=[tab_declaration, tab_push],
    )


def test_make_ast_call_function_without_parameters() -> None:
    lexer, parser = init()
    call_expr = CallExpression(
        kind=NodeType.CALL_EXPRESSION,
        callee=Identifier(kind=NodeType.IDENTIFIER, identifier="abc"),
        arguments=[],
    )

    assert make_ast_from_input("abc()", lexer, parser) == Program(
        kind=NodeType.PROGAM,
        body=[call_expr],
    )


def test_make_ast_call_function_with_parameters() -> None:
    lexer, parser = init()
    call_expr = CallExpression(
        kind=NodeType.CALL_EXPRESSION,
        callee=Identifier(kind=NodeType.IDENTIFIER, identifier="abc"),
        arguments=[
            NumericLiteral(kind=NodeType.NUMERIC_LITERAL, value=1),
            StringLiteral(kind=NodeType.STRING_LITERAL, value="Hello, World!"),
            Identifier(kind=NodeType.IDENTIFIER, identifier="code"),
        ],
    )

    assert make_ast_from_input(
        'abc(1, "Hello, World!", code)', lexer, parser
    ) == Program(
        kind=NodeType.PROGAM,
        body=[call_expr],
    )


def test_make_ast_declare_function_without_parameters() -> None:
    lexer, parser = init()
    func = FunctionStatement(
        kind=NodeType.FUNCTION_STATEMENT,
        identifier="abc",
        parameters=[],
        body=BlockStatement(
            kind=NodeType.BLOCK_STATEMENT,
            body=[
                PrintStatement(
                    kind=NodeType.PRINT_STATEMENT,
                    value=NumericLiteral(kind=NodeType.NUMERIC_LITERAL, value=42),
                )
            ],
        ),
    )

    input = [
        "fonction abc()",
        "  ecrire 42",
        "fin",
    ]
    assert make_ast_from_input("\n".join(input), lexer, parser) == Program(
        kind=NodeType.PROGAM,
        body=[func],
    )


def test_make_ast_declare_function_with_parameters() -> None:
    lexer, parser = init()
    body_func = BlockStatement(
        kind=NodeType.BLOCK_STATEMENT,
        body=[
            PrintStatement(
                kind=NodeType.PRINT_STATEMENT,
                value=Identifier(kind=NodeType.IDENTIFIER, identifier="a"),
            ),
            PrintStatement(
                kind=NodeType.PRINT_STATEMENT,
                value=Identifier(kind=NodeType.IDENTIFIER, identifier="b"),
            ),
            PrintStatement(
                kind=NodeType.PRINT_STATEMENT,
                value=Identifier(kind=NodeType.IDENTIFIER, identifier="c"),
            ),
        ],
    )
    func = FunctionStatement(
        kind=NodeType.FUNCTION_STATEMENT,
        identifier="abc",
        parameters=["a", "b", "c"],
        body=body_func,
    )

    input = [
        "fonction abc(a, b, c)",
        "  ecrire a",
        "  ecrire b",
        "  ecrire c",
        "fin",
    ]
    assert make_ast_from_input("\n".join(input), lexer, parser) == Program(
        kind=NodeType.PROGAM,
        body=[func],
    )


def test_make_ast_variable_is_an_anonymous_function() -> None:
    lexer, parser = init()
    body_func = BlockStatement(
        kind=NodeType.BLOCK_STATEMENT,
        body=[
            PrintStatement(
                kind=NodeType.PRINT_STATEMENT,
                value=Identifier(kind=NodeType.IDENTIFIER, identifier="a"),
            ),
            PrintStatement(
                kind=NodeType.PRINT_STATEMENT,
                value=Identifier(kind=NodeType.IDENTIFIER, identifier="b"),
            ),
            PrintStatement(
                kind=NodeType.PRINT_STATEMENT,
                value=Identifier(kind=NodeType.IDENTIFIER, identifier="c"),
            ),
        ],
    )
    func = FunctionStatement(
        kind=NodeType.FUNCTION_STATEMENT,
        identifier=None,
        parameters=["a", "b", "c"],
        body=body_func,
    )

    input = [
        "dec abc = fonction(a, b, c)",
        "  ecrire a",
        "  ecrire b",
        "  ecrire c",
        "fin",
    ]
    assert make_ast_from_input("\n".join(input), lexer, parser) == Program(
        kind=NodeType.PROGAM,
        body=[
            VariableDeclarationStatement(
                kind=NodeType.VARIABLE_DECLARATION_STATEMENT,
                identifier="abc",
                value=func,
            )
        ],
    )


def test_make_ast_call_with_callback() -> None:
    lexer, parser = init()
    body_func = BlockStatement(
        kind=NodeType.BLOCK_STATEMENT,
        body=[
            PrintStatement(
                kind=NodeType.PRINT_STATEMENT,
                value=Identifier(kind=NodeType.IDENTIFIER, identifier="a"),
            ),
        ],
    )
    func = FunctionStatement(
        kind=NodeType.FUNCTION_STATEMENT,
        identifier=None,
        parameters=["a"],
        body=body_func,
    )
    call_expr = CallExpression(
        kind=NodeType.CALL_EXPRESSION,
        callee=Identifier(kind=NodeType.IDENTIFIER, identifier="abc"),
        arguments=[func],
    )

    input = [
        "abc(fonction(a)",
        "  ecrire a",
        "fin)",
    ]
    assert make_ast_from_input("\n".join(input), lexer, parser) == Program(
        kind=NodeType.PROGAM,
        body=[call_expr],
    )


def test_make_ast_multiple_member_access_and_call() -> None:
    lexer, parser = init()
    global_access = MemberExpression(
        kind=NodeType.MEMBER_EXPRESSION,
        object=Identifier(kind=NodeType.IDENTIFIER, identifier="globals"),
        property=NumericLiteral(kind=NodeType.NUMERIC_LITERAL, value=0),
    )
    call_a = CallExpression(
        kind=NodeType.CALL_EXPRESSION,
        callee=global_access,
        arguments=[Identifier(kind=NodeType.IDENTIFIER, identifier="a")],
    )
    array_a_access = MemberExpression(
        kind=NodeType.MEMBER_EXPRESSION,
        object=call_a,
        property=NumericLiteral(kind=NodeType.NUMERIC_LITERAL, value=1),
    )
    call_b = CallExpression(
        kind=NodeType.CALL_EXPRESSION,
        callee=array_a_access,
        arguments=[Identifier(kind=NodeType.IDENTIFIER, identifier="b")],
    )
    call_c = CallExpression(
        kind=NodeType.CALL_EXPRESSION,
        callee=call_b,
        arguments=[Identifier(kind=NodeType.IDENTIFIER, identifier="c")],
    )
    array_c_access = MemberExpression(
        kind=NodeType.MEMBER_EXPRESSION,
        object=call_c,
        property=NumericLiteral(kind=NodeType.NUMERIC_LITERAL, value=2),
    )
    call_d = CallExpression(
        kind=NodeType.CALL_EXPRESSION,
        callee=array_c_access,
        arguments=[],
    )

    assert make_ast_from_input(
        "(((((globals[0])(a))[1])(b)(c))[2])()", lexer, parser
    ) == Program(
        kind=NodeType.PROGAM,
        body=[call_d],
    )
