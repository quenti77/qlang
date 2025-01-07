import pytest  # type: ignore

from src.interpreter import Interpreter
from src.lexer import Lexer
from src.parser import Parser
from src.runtime.environment import Environment
from src.runtime.std import Std
from src.runtime.values import MK_ARRAY
from src.runtime.values import MK_BOOLEAN
from src.runtime.values import MK_NULL
from src.runtime.values import MK_NUMBER
from src.runtime.values import RuntimeValue


def interpret(code: str) -> tuple[RuntimeValue, Std, Std, Environment]:
    lexer = Lexer()
    lexer.tokenize(code)

    parser = Parser()
    parser.setTokens(lexer.tokens, code)
    ast = parser.make_ast()

    env = Environment(None)
    stdOut = Std()
    stdErr = Std()
    interpreter = Interpreter(env, stdOut, stdErr)

    return interpreter.evaluate(ast), stdOut, stdErr, env


def test_empty_program() -> None:
    response, _, _, _ = interpret("")
    assert response == MK_NULL()


def test_print() -> None:
    response, stdOut, _, _ = interpret('ecrire "Salut"')
    assert response == MK_NULL()
    assert stdOut.log == ["Salut"]


def test_priority_numeric() -> None:
    response, _, _, _ = interpret("2 + 3 * 4")
    assert response == MK_NUMBER(14)


def test_parenthesis_priority() -> None:
    response, _, _, _ = interpret("(2 + 3) * 4")
    assert response == MK_NUMBER(20)


@pytest.mark.parametrize(
    "code, expected",
    [
        ("vrai", MK_BOOLEAN(True)),
        ("faux", MK_BOOLEAN(False)),
    ],
)
def test_boolean(code: str, expected: RuntimeValue) -> None:
    response, _, _, _ = interpret(code)
    assert response == expected


def test_variable() -> None:
    response, _, _, env = interpret("dec age = 20")

    assert env.lookupVariable("age") == MK_NUMBER(20)
    assert response == MK_NUMBER(20)


def test_variable_reassign() -> None:
    response, _, _, env = interpret("dec age = 20\nage = 30")

    assert env.lookupVariable("age") == MK_NUMBER(30)
    assert response == MK_NUMBER(30)


def test_variable_mass_reassign() -> None:
    response, _, _, env = interpret(
        """
        dec a = 20
        dec b = 30
        dec c = 40
        dec d = 50
        dec e = 60
        a = b = c = d = 70
        """
    )

    assert env.lookupVariable("a") == MK_NUMBER(70)
    assert env.lookupVariable("b") == MK_NUMBER(70)
    assert env.lookupVariable("c") == MK_NUMBER(70)
    assert env.lookupVariable("d") == MK_NUMBER(70)
    assert env.lookupVariable("e") == MK_NUMBER(60)
    assert response == MK_NUMBER(70)


def test_member_assignment() -> None:
    response, _, _, env = interpret(
        """
        dec tab = [10, [20, 30]]
        tab[1][0] = 40
        """
    )

    assert env.lookupVariable("tab") == MK_ARRAY(
        [MK_NUMBER(10), MK_ARRAY([MK_NUMBER(40), MK_NUMBER(30)])]
    )
    assert response == MK_NUMBER(40)


def test_function_return_function() -> None:
    response, _, _, _ = interpret(
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
