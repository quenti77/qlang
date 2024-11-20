from src.enums.token_type import TOKEN_TYPE


KEYWORDS = {
    "dec": TOKEN_TYPE.LET,
    "si": TOKEN_TYPE.IF,
    "pour": TOKEN_TYPE.FOR,
    "de": TOKEN_TYPE.FROM,
    "jusque": TOKEN_TYPE.UNTIL,
    "evol": TOKEN_TYPE.STEP,
    "tantque": TOKEN_TYPE.WHILE,
    "alors": TOKEN_TYPE.THEN,
    "sinon": TOKEN_TYPE.ELSE,
    "sinonsi": TOKEN_TYPE.ELSE_IF,
    "fin": TOKEN_TYPE.END,
    "arreter": TOKEN_TYPE.BREAK,
    "continuer": TOKEN_TYPE.CONTINUE,
    "retour": TOKEN_TYPE.RETURN,
    "rien": TOKEN_TYPE.NULL,
    "vrai": TOKEN_TYPE.BOOLEAN,
    "faux": TOKEN_TYPE.BOOLEAN,
    "lire": TOKEN_TYPE.READ,
    "ecrire": TOKEN_TYPE.PRINT,
    "et": TOKEN_TYPE.BINARY_OPERATOR,
    "ou": TOKEN_TYPE.BINARY_OPERATOR,
    "non": TOKEN_TYPE.UNARY_OPERATOR,
    "fonction": TOKEN_TYPE.FUNCTION,
}


def find_keyword_from_token(token: TOKEN_TYPE) -> list[str]:
    keywords = []
    for keyword, token_type in KEYWORDS.items():
        if token_type == token:
            keywords.append(keyword)
    return keywords
