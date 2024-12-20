from src.runtime.environment import Environment
from src.runtime.values import MK_NUMBER


def init(parent: Environment | None) -> Environment:
    return Environment(parent)


def test_declare_only_one() -> None:
    env = init(None)

    env.declareVariable("a", MK_NUMBER(1))
    assert env.lookupVariable("a") == MK_NUMBER(1)


def test_declare_multiple() -> None:
    env = init(None)

    env.declareVariable("a", MK_NUMBER(1))
    env.declareVariable("b", MK_NUMBER(2))
    env.declareVariable("c", MK_NUMBER(3))

    assert env.lookupVariable("a") == MK_NUMBER(1)
    assert env.lookupVariable("b") == MK_NUMBER(2)
    assert env.lookupVariable("c") == MK_NUMBER(3)


def test_same_name() -> None:
    env = init(None)

    env.declareVariable("a", MK_NUMBER(1))
    try:
        env.declareVariable("a", MK_NUMBER(2))
    except RuntimeError as e:
        assert str(e) == "Variable 'a' déjà déclarée"


def test_assign() -> None:
    env = init(None)

    env.declareVariable("a", MK_NUMBER(1))
    env.assignVariable("a", MK_NUMBER(2))

    assert env.lookupVariable("a") == MK_NUMBER(2)


def test_assign_not_declared() -> None:
    env = init(None)

    try:
        env.assignVariable("a", MK_NUMBER(2))
    except RuntimeError as e:
        assert str(e) == "Variable 'a' non déclarée"


def test_declare_in_child() -> None:
    parent = init(None)
    child = init(parent)

    parent.declareVariable("a", MK_NUMBER(1))
    child.declareVariable("b", MK_NUMBER(2))

    assert child.lookupVariable("a") == MK_NUMBER(1)
    assert child.lookupVariable("b") == MK_NUMBER(2)


def test_override_in_child() -> None:
    parent = init(None)
    child = init(parent)

    parent.declareVariable("a", MK_NUMBER(1))
    child.declareVariable("a", MK_NUMBER(2))

    assert child.lookupVariable("a") == MK_NUMBER(2)
    assert parent.lookupVariable("a") == MK_NUMBER(1)


def test_assign_in_parent_from_child() -> None:
    parent = init(None)
    child = init(parent)

    parent.declareVariable("a", MK_NUMBER(1))
    child.assignVariable("a", MK_NUMBER(2))

    assert child.lookupVariable("a") == MK_NUMBER(2)
    assert parent.lookupVariable("a") == MK_NUMBER(2)
