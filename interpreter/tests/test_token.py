import pytest

from src.enums.keywords import KEYWORDS
from src.enums.operators import OPERATORS
from src.enums.token_type import TOKEN_TYPE
from src.lexer import Lexer
from src.token import create_token_at
from tests.tools import assert_array_equals


def init() -> Lexer:
    return Lexer()


def test_tokenize_float() -> None:
    lexer = init()
    lexer.tokenize("1.2")
    tokens = lexer.get_tokens()

    assert assert_array_equals(
        [
            create_token_at(TOKEN_TYPE.NUMBER, "1.2", 1, 1, 1),
            create_token_at(TOKEN_TYPE.EOF, "", 4, 1, 4),
        ],
        tokens,
    )


def test_tokenize_a_simple_math_expression() -> None:
    lexer = init()
    lexer.tokenize("40 + 20 * 60 - 40 / 30")
    tokens = lexer.get_tokens()

    assert assert_array_equals(
        [
            create_token_at(TOKEN_TYPE.NUMBER, "40", 1, 1, 1),
            create_token_at(TOKEN_TYPE.BINARY_OPERATOR, "+", 4, 1, 4),
            create_token_at(TOKEN_TYPE.NUMBER, "20", 6, 1, 6),
            create_token_at(TOKEN_TYPE.BINARY_OPERATOR, "*", 9, 1, 9),
            create_token_at(TOKEN_TYPE.NUMBER, "60", 11, 1, 11),
            create_token_at(TOKEN_TYPE.BINARY_OPERATOR, "-", 14, 1, 14),
            create_token_at(TOKEN_TYPE.NUMBER, "40", 16, 1, 16),
            create_token_at(TOKEN_TYPE.BINARY_OPERATOR, "/", 19, 1, 19),
            create_token_at(TOKEN_TYPE.NUMBER, "30", 21, 1, 21),
            create_token_at(TOKEN_TYPE.EOF, "", 23, 1, 23),
        ],
        tokens,
    )


def test_parenthesis_expression() -> None:
    lexer = init()
    lexer.tokenize("(40 + 20)")
    tokens = lexer.get_tokens()

    assert assert_array_equals(
        [
            create_token_at(TOKEN_TYPE.OPEN_PARENTHESIS, "(", 1, 1, 1),
            create_token_at(TOKEN_TYPE.NUMBER, "40", 2, 1, 2),
            create_token_at(TOKEN_TYPE.BINARY_OPERATOR, "+", 5, 1, 5),
            create_token_at(TOKEN_TYPE.NUMBER, "20", 7, 1, 7),
            create_token_at(TOKEN_TYPE.CLOSE_PARENTHESIS, ")", 9, 1, 9),
            create_token_at(TOKEN_TYPE.EOF, "", 10, 1, 10),
        ],
        tokens,
    )


def test_tokenize_a_simple_affectation() -> None:
    lexer = init()
    lexer.tokenize("abc_123 = 41 + 23")
    tokens = lexer.get_tokens()

    assert assert_array_equals(
        [
            create_token_at(TOKEN_TYPE.IDENTIFIER, "abc_123", 1, 1, 1),
            create_token_at(TOKEN_TYPE.EQUALS, "=", 9, 1, 9),
            create_token_at(TOKEN_TYPE.NUMBER, "41", 11, 1, 11),
            create_token_at(TOKEN_TYPE.BINARY_OPERATOR, "+", 14, 1, 14),
            create_token_at(TOKEN_TYPE.NUMBER, "23", 16, 1, 16),
            create_token_at(TOKEN_TYPE.EOF, "", 18, 1, 18),
        ],
        tokens,
    )


def test_tokenize_a_simple_affectation_with_underscore_variable() -> None:
    lexer = init()
    lexer.tokenize("_abc = 41 + 23")
    tokens = lexer.get_tokens()

    assert assert_array_equals(
        [
            create_token_at(TOKEN_TYPE.IDENTIFIER, "_abc", 1, 1, 1),
            create_token_at(TOKEN_TYPE.EQUALS, "=", 6, 1, 6),
            create_token_at(TOKEN_TYPE.NUMBER, "41", 8, 1, 8),
            create_token_at(TOKEN_TYPE.BINARY_OPERATOR, "+", 11, 1, 11),
            create_token_at(TOKEN_TYPE.NUMBER, "23", 13, 1, 13),
            create_token_at(TOKEN_TYPE.EOF, "", 15, 1, 15),
        ],
        tokens,
    )


def test_a_multiline_input() -> None:
    lexer = init()

    input = [
        "a = 1 + 2",
        "b = 3 * 4",
        "c = a + b",
    ]
    lexer.tokenize("\n".join(input))
    tokens = lexer.get_tokens()

    assert assert_array_equals(
        [
            create_token_at(TOKEN_TYPE.IDENTIFIER, "a", 1, 1, 1),
            create_token_at(TOKEN_TYPE.EQUALS, "=", 3, 1, 3),
            create_token_at(TOKEN_TYPE.NUMBER, "1", 5, 1, 5),
            create_token_at(TOKEN_TYPE.BINARY_OPERATOR, "+", 7, 1, 7),
            create_token_at(TOKEN_TYPE.NUMBER, "2", 9, 1, 9),
            create_token_at(TOKEN_TYPE.IDENTIFIER, "b", 11, 2, 1),
            create_token_at(TOKEN_TYPE.EQUALS, "=", 13, 2, 3),
            create_token_at(TOKEN_TYPE.NUMBER, "3", 15, 2, 5),
            create_token_at(TOKEN_TYPE.BINARY_OPERATOR, "*", 17, 2, 7),
            create_token_at(TOKEN_TYPE.NUMBER, "4", 19, 2, 9),
            create_token_at(TOKEN_TYPE.IDENTIFIER, "c", 21, 3, 1),
            create_token_at(TOKEN_TYPE.EQUALS, "=", 23, 3, 3),
            create_token_at(TOKEN_TYPE.IDENTIFIER, "a", 25, 3, 5),
            create_token_at(TOKEN_TYPE.BINARY_OPERATOR, "+", 27, 3, 7),
            create_token_at(TOKEN_TYPE.IDENTIFIER, "b", 29, 3, 9),
            create_token_at(TOKEN_TYPE.EOF, "", 30, 3, 10),
        ],
        tokens,
    )


@pytest.mark.parametrize("keyword", KEYWORDS.items())
def test_tokenize_keywords(keyword: tuple[str, TOKEN_TYPE]) -> None:
    keyword_name = keyword[0]
    keyword_len = len(keyword_name)
    token_type = keyword[1]

    lexer = init()
    lexer.tokenize(f"{keyword_name} + a + 2")
    tokens = lexer.get_tokens()

    assert assert_array_equals(
        [
            create_token_at(token_type, keyword_name, 1, 1, 1),
            create_token_at(
                TOKEN_TYPE.BINARY_OPERATOR, "+", keyword_len + 2, 1, keyword_len + 2
            ),
            create_token_at(
                TOKEN_TYPE.IDENTIFIER, "a", keyword_len + 4, 1, keyword_len + 4
            ),
            create_token_at(
                TOKEN_TYPE.BINARY_OPERATOR, "+", keyword_len + 6, 1, keyword_len + 6
            ),
            create_token_at(
                TOKEN_TYPE.NUMBER, "2", keyword_len + 8, 1, keyword_len + 8
            ),
            create_token_at(TOKEN_TYPE.EOF, "", keyword_len + 9, 1, keyword_len + 9),
        ],
        tokens,
    )


@pytest.mark.parametrize("operator", OPERATORS)
def test_tokenize_an_operator(operator) -> None:
    lexer = init()
    lexer.tokenize(f"a {operator} 2")
    tokens = lexer.get_tokens()

    operator_len = len(operator)

    assert assert_array_equals(
        [
            create_token_at(TOKEN_TYPE.IDENTIFIER, "a", 1, 1, 1),
            create_token_at(TOKEN_TYPE.BINARY_OPERATOR, operator, 3, 1, 3),
            create_token_at(
                TOKEN_TYPE.NUMBER, "2", operator_len + 4, 1, operator_len + 4
            ),
            create_token_at(TOKEN_TYPE.EOF, "", operator_len + 5, 1, operator_len + 5),
        ],
        tokens,
    )


def test_tokenize_simple_string() -> None:
    lexer = init()
    lexer.tokenize('"hello world"')
    tokens = lexer.get_tokens()

    assert assert_array_equals(
        [
            create_token_at(TOKEN_TYPE.STRING, "hello world", 2, 1, 2),
            create_token_at(TOKEN_TYPE.EOF, "", 14, 1, 14),
        ],
        tokens,
    )


def test_tokenize_escaped_string() -> None:
    lexer = init()
    lexer.tokenize(r'"\\\thello\n\"world\""')
    tokens = lexer.get_tokens()

    assert assert_array_equals(
        [
            create_token_at(TOKEN_TYPE.STRING, '\\\thello\n"world"', 2, 1, 2),
            create_token_at(TOKEN_TYPE.EOF, "", 18, 1, 18),
        ],
        tokens,
    )


def test_tokenize_multiline_string() -> None:
    lexer = init()

    input = [
        '"Hello',
        "the",
        'world"',
    ]
    lexer.tokenize("\n".join(input))
    tokens = lexer.get_tokens()

    assert assert_array_equals(
        [
            create_token_at(TOKEN_TYPE.STRING, "Hello\nthe\nworld", 2, 1, 2),
            create_token_at(TOKEN_TYPE.EOF, "", 18, 3, 7),
        ],
        tokens,
    )


def test_remove_comment() -> None:
    lexer = init()

    input = [
        "rem The next lineis an affectation",
        "a = 1 + 2 rem a take the value 3",
    ]
    lexer.tokenize("\n".join(input))
    tokens = lexer.get_tokens()

    assert assert_array_equals(
        [
            create_token_at(TOKEN_TYPE.IDENTIFIER, "a", 5, 2, 1),
            create_token_at(TOKEN_TYPE.EQUALS, "=", 7, 2, 3),
            create_token_at(TOKEN_TYPE.NUMBER, "1", 9, 2, 5),
            create_token_at(TOKEN_TYPE.BINARY_OPERATOR, "+", 11, 2, 7),
            create_token_at(TOKEN_TYPE.NUMBER, "2", 13, 2, 9),
            create_token_at(TOKEN_TYPE.EOF, "", 18, 2, 14),
        ],
        tokens,
    )


def test_all_code_with_comment_returns_empty() -> None:
    lexer = init()

    input = [
        "rem The next lineis an affectation",
        "rem a take the value 3",
        "rem another line",
    ]
    lexer.tokenize("\n".join(input))
    tokens = lexer.get_tokens()

    assert assert_array_equals(
        [create_token_at(TOKEN_TYPE.EOF, "", 12, 3, 4)],
        tokens,
    )


def test_tokenize_unary_operator() -> None:
    lexer = init()
    lexer.tokenize("-1")
    tokens = lexer.get_tokens()

    assert assert_array_equals(
        [
            create_token_at(TOKEN_TYPE.UNARY_OPERATOR, "-", 1, 1, 1),
            create_token_at(TOKEN_TYPE.NUMBER, "1", 2, 1, 2),
            create_token_at(TOKEN_TYPE.EOF, "", 3, 1, 3),
        ],
        tokens,
    )


def test_tokenize_unary_operator_with_parenthesis() -> None:
    lexer = init()
    lexer.tokenize("-(1 + 2)")
    tokens = lexer.get_tokens()

    assert assert_array_equals(
        [
            create_token_at(TOKEN_TYPE.UNARY_OPERATOR, "-", 1, 1, 1),
            create_token_at(TOKEN_TYPE.OPEN_PARENTHESIS, "(", 2, 1, 2),
            create_token_at(TOKEN_TYPE.NUMBER, "1", 3, 1, 3),
            create_token_at(TOKEN_TYPE.BINARY_OPERATOR, "+", 5, 1, 5),
            create_token_at(TOKEN_TYPE.NUMBER, "2", 7, 1, 7),
            create_token_at(TOKEN_TYPE.CLOSE_PARENTHESIS, ")", 8, 1, 8),
            create_token_at(TOKEN_TYPE.EOF, "", 9, 1, 9),
        ],
        tokens,
    )


def test_tokenize_unary_with_binary() -> None:
    lexer = init()
    lexer.tokenize("1 --a")
    tokens = lexer.get_tokens()

    assert assert_array_equals(
        [
            create_token_at(TOKEN_TYPE.NUMBER, "1", 1, 1, 1),
            create_token_at(TOKEN_TYPE.BINARY_OPERATOR, "-", 3, 1, 3),
            create_token_at(TOKEN_TYPE.UNARY_OPERATOR, "-", 4, 1, 4),
            create_token_at(TOKEN_TYPE.IDENTIFIER, "a", 5, 1, 5),
            create_token_at(TOKEN_TYPE.EOF, "", 6, 1, 6),
        ],
        tokens,
    )


def test_tokenize_array_elements() -> None:
    lexer = init()
    lexer.tokenize("[1, 2, 3]")
    tokens = lexer.get_tokens()

    assert assert_array_equals(
        [
            create_token_at(TOKEN_TYPE.OPEN_BRACKET, "[", 1, 1, 1),
            create_token_at(TOKEN_TYPE.NUMBER, "1", 2, 1, 2),
            create_token_at(TOKEN_TYPE.COMMA, ",", 3, 1, 3),
            create_token_at(TOKEN_TYPE.NUMBER, "2", 5, 1, 5),
            create_token_at(TOKEN_TYPE.COMMA, ",", 6, 1, 6),
            create_token_at(TOKEN_TYPE.NUMBER, "3", 8, 1, 8),
            create_token_at(TOKEN_TYPE.CLOSE_BRACKET, "]", 9, 1, 9),
            create_token_at(TOKEN_TYPE.EOF, "", 10, 1, 10),
        ],
        tokens,
    )
