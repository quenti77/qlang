from enum import Enum


class NodeType(Enum):
    # Statements
    PROGAM = "Program"
    VARIABLE_DECLARATION_STATEMENT = "VariableDeclarationStatement"
    PRINT_STATEMENT = "PrintStatement"
    IF_STATEMENT = "IfStatement"
    WHILE_STATEMENT = "WhileStatement"
    FOR_STATEMENT = "ForStatement"
    FUNCTION_STATEMENT = "FunctionStatement"
    BLOCK_STATEMENT = "BlockStatement"
    BREAK_STATEMENT = "BreakStatement"
    CONTINUE_STATEMENT = "ContinueStatement"
    RETURN_STATEMENT = "ReturnStatement"
    # Expressions
    ASSIGNMENT_EXPRESSION = "AssignmentExpression"
    UNARY_EXPRESSION = "UnaryExpression"
    BINARY_EXPRESSION = "BinaryExpression"
    NUMERIC_LITERAL = "NumericLiteral"
    STRING_LITERAL = "StringLiteral"
    NULL_LITERAL = "NullLiteral"
    BOOLEAN_LITERAL = "BooleanLiteral"
    IDENTIFIER = "Identifier"
    MEMBER_CALL_EXPRESSION = "MemberCallExpression"
    ARRAY_EXPRESSION = "ArrayExpression"
    CALL_EXPRESSION = "CallExpression"
