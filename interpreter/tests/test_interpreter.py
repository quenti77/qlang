import pytest  # type: ignore

from src.interpreter import Interpreter
from src.lexer import Lexer
from src.nodes.statement import WhileStatement
from src.parser import Parser
from src.runtime.environment import Environment
from src.runtime.std import Std
from src.runtime.values import MK_ARRAY
from src.runtime.values import MK_BOOLEAN
from src.runtime.values import MK_BREAK
from src.runtime.values import MK_CONTINUE
from src.runtime.values import MK_NULL
from src.runtime.values import MK_NUMBER
from src.runtime.values import MK_RETURN
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


def test_unary_expression() -> None:
    response, _, _, _ = interpret("-2")
    assert response == MK_NUMBER(-2)


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


def test_expression_with_unary() -> None:
    response, _, _, env = interpret(
        """
        dec a = -2
        dec b = -a
        dec c = -a - -b
        """
    )
    assert response == MK_NUMBER(4)
    assert env.lookupVariable("a") == MK_NUMBER(-2)
    assert env.lookupVariable("b") == MK_NUMBER(2)
    assert env.lookupVariable("c") == MK_NUMBER(4)


def test_full_condition() -> None:
    response, out, _, _ = interpret(
        """
        dec a = 17
        si a >= 21 alors
            ecrire "a >= 21"
        sinonsi a >= 18 alors
            ecrire "a >= 18"
        sinon
            ecrire "a < 18"
        fin
        """
    )
    assert response == MK_NULL()
    assert out.log == ["a < 18"]


def test_short_circuit_and_condition() -> None:
    response, out, _, _ = interpret(
        """
        fonction a()
            ecrire "a"
            retour faux
        fin

        fonction b()
            ecrire "b"
            retour vrai
        fin

        si a() et b() alors
            ecrire "Vrai"
        fin
        """
    )
    assert response == MK_NULL()
    assert out.log == ["a"]


def test_short_circuit_or_condition() -> None:
    response, out, _, _ = interpret(
        """
        fonction a()
            ecrire "a"
            retour vrai
        fin

        fonction b()
            ecrire "b"
            retour faux
        fin

        si a() ou b() alors
            ecrire "Vrai"
        fin
        """
    )
    assert response == MK_NULL()
    assert out.log == ["a", "Vrai"]


def test_loop_for() -> None:
    response, out, _, _ = interpret(
        """
        pour i de 0 jusque 5 alors
            ecrire i
        fin
        """
    )
    assert response == MK_NULL()
    assert out.log == ["0.0", "1.0", "2.0", "3.0", "4.0", "5.0"]


def test_loop_for_with_step() -> None:
    response, out, _, _ = interpret(
        """
        pour i de 0 jusque 5 evol 2 alors
            ecrire i
        fin
        """
    )
    assert response == MK_NULL()
    assert out.log == ["0.0", "2.0", "4.0"]


def test_loop_for_with_negative_step() -> None:
    response, out, _, _ = interpret(
        """
        pour i de 5 jusque i >= 0 evol -2 alors
            ecrire i
        fin
        """
    )
    assert response == MK_NULL()
    assert out.log == ["5.0", "3.0", "1.0"]


def test_loop_while() -> None:
    response, out, _, _ = interpret(
        """
        dec i = 0
        tantque i < 5 alors
            ecrire i
            i = i + 1
        fin
        """
    )
    assert response == MK_NULL()
    assert out.log == ["0.0", "1.0", "2.0", "3.0", "4.0"]


@pytest.mark.parametrize(
    "code, expected",
    [
        ("arreter", MK_BREAK()),
        ("continuer", MK_CONTINUE()),
        ("retour 42", MK_RETURN(MK_NUMBER(42))),
    ],
)
def test_break_program(code: str, expected: RuntimeValue) -> None:
    code = f"""
        {code}
        """
    lexer = Lexer()
    lexer.tokenize(code)

    parser = Parser()
    parser.setTokens(lexer.tokens, code)
    ast = parser.make_ast()

    env = Environment(None)
    stdOut = Std()
    stdErr = Std()
    interpreter = Interpreter(env, stdOut, stdErr)
    print(ast)
    assert interpreter.evaluate(ast) == expected


@pytest.mark.parametrize(
    "code, expected",
    [
        ("arreter", MK_BREAK()),
        ("continuer", MK_CONTINUE()),
        ("retour 42", MK_RETURN(MK_NUMBER(42))),
    ],
)
def test_break_loop(code: str, expected: RuntimeValue) -> None:
    code = f"""
        tantque vrai alors
            si vrai alors
                si vrai alors
                    {code}
                fin
            fin
        fin
        """
    lexer = Lexer()
    lexer.tokenize(code)

    parser = Parser()
    parser.setTokens(lexer.tokens, code)
    ast = parser.make_ast()
    while_statement: WhileStatement = ast.body[0]  # type: ignore

    env = Environment(None)
    stdOut = Std()
    stdErr = Std()
    interpreter = Interpreter(env, stdOut, stdErr)
    assert interpreter.evaluate(while_statement.body) == expected


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
