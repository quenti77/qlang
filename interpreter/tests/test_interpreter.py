import pytest  # type: ignore

from src.interpreter import Interpreter
from src.lexer import Lexer
from src.parser import Parser
from src.runtime.environment import Environment
from src.runtime.std import Std
from src.runtime.values import MK_BOOLEAN
from src.runtime.values import MK_NULL
from src.runtime.values import MK_NUMBER
from src.runtime.values import RuntimeValue


def interpret(code: str) -> tuple[RuntimeValue, Std, Std]:
    lexer = Lexer()
    lexer.tokenize(code)

    parser = Parser()
    parser.setTokens(lexer.tokens, code)
    ast = parser.make_ast()

    env = Environment(None)
    stdOut = Std()
    stdErr = Std()
    interpreter = Interpreter(env, stdOut, stdErr)

    return interpreter.evaluate(ast), stdOut, stdErr


def test_empty_program() -> None:
    response, _, _ = interpret("")
    assert response == MK_NULL()


def test_print() -> None:
    response, stdOut, _ = interpret('ecrire "Salut"')
    assert response == MK_NULL()
    assert stdOut.log == ["Salut"]


def test_priority_numeric() -> None:
    response, _, _ = interpret("2 + 3 * 4")
    assert response == MK_NUMBER(14)


def test_parenthesis_priority() -> None:
    response, _, _ = interpret("(2 + 3) * 4")
    assert response == MK_NUMBER(20)


@pytest.mark.parametrize(
    "code, expected",
    [
        ("vrai", MK_BOOLEAN(True)),
        ("faux", MK_BOOLEAN(False)),
    ],
)
def test_boolean(code: str, expected: RuntimeValue) -> None:
    response, _, _ = interpret(code)
    assert response == expected


def test_function_return_function() -> None:
    response, _, _ = interpret(
        """
        fonction first(val)
            fonction second(mul)
                retour val * mul
            fin
            retour second
        fin

        first(20)(30)
        """
    )
    assert response == MK_NUMBER(600)
