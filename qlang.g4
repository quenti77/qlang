grammar qlang;

// ====================================================================
// qlang — grammaire ANTLR
// ====================================================================
// Cette grammaire est INDICATIVE : elle donne un aperçu de la syntaxe.
// L'implémentation de référence est le lexer et le parseur écrits à la main
// dans crates/qlang-core/src (lexer.rs, parser.rs), et la description du
// langage est dans docs/language.md. En cas de désaccord, ce sont eux qui
// font foi.

program: Newline* (topLevelItem (Newline+ topLevelItem)* Newline*)? EOF;

// ====================================================================
// PARSER — Expressions (section 2)
// ====================================================================
// Précédence, de la plus faible à la plus forte :
//   =  += -= *= /= %=      (droite)
//   .. ..=                 (non associatif)
//   or
//   and
//   not                    (préfixe)
//   == != < <= > >=        (non associatif : a < b < c est interdit)
//   + -
//   * / div mod %          (% est un synonyme de mod)
//   as                     (conversion : x as T appelle As<T>.convert)
//   -x                     (préfixe)
//   **                     (droite : -2 ** 2 vaut -(2 ** 2))
//   f()  a[i]  a.b         (suffixes)
//
// Les retours à la ligne sont ignorés après un opérateur binaire, une
// virgule et dans les (), [] et {} : c'est le rôle des Newline*.
// La cible d'une affectation (identifiant, a.b, a[i]) est vérifiée
// sémantiquement, pas par la grammaire.

expression: assignmentExpression;

assignmentExpression
    : rangeExpression (assignOperator Newline* assignmentExpression)?
    ;

assignOperator
    : Assign | PlusAssign | MinusAssign | StarAssign | SlashAssign | PercentAssign
    | IntDivAssign | ModAssign
    ;

rangeExpression
    : orExpression ((DotDot | DotDotEq) Newline* orExpression)?
    ;

orExpression: andExpression (Or Newline* andExpression)*;

andExpression: notExpression (And Newline* notExpression)*;

notExpression
    : Not notExpression
    | comparisonExpression
    ;

comparisonExpression
    : additiveExpression
      ((EqEq | NotEq | Lt | LtEq | Gt | GtEq) Newline* additiveExpression)?
    ;

additiveExpression
    : multiplicativeExpression ((Plus | Minus) Newline* multiplicativeExpression)*
    ;

multiplicativeExpression
    : castExpression ((Star | Slash | DivKw | ModKw | Percent) Newline* castExpression)*
    ;

castExpression: unaryExpression (As type)*;

unaryExpression
    : Minus unaryExpression
    | powerExpression
    ;

// l'opérande de droite est une unaryExpression : 2 ** 3 ** 2 vaut 2 ** (3 ** 2)
powerExpression: postfixExpression (Power Newline* unaryExpression)?;

// appel f(x), appel générique f<int>(x), index a[i], membre a.b, a.b()
// Un '<' est un début d'arguments de type seulement si la liste de types
// est fermée par '>' puis suivie de '(' ou '.' (même règle que TypeScript).
postfixExpression
    : primaryExpression
      ( typeArguments? '(' Newline* argumentList? Newline* ')'
      | typeArguments? Dot Identifier
      | '[' Newline* expression Newline* ']'
      // a[] n'est valide que comme cible d'une affectation : a[] = valeur
      | '[' Newline* ']'
      )*
    ;

argumentList: expression (Comma Newline* expression)* Comma?;

primaryExpression
    : literal
    | Identifier
    | Self
    | Super
    | '(' Newline* expression Newline* ')'
    | arrayLiteral
    | mapLiteral
    | structLiteral
    | ifExpression
    | matchExpression
    | functionExpression
    ;

literal
    : IntLiteral
    | FloatLiteral
    | StringLiteral
    | True
    | False
    | None
    ;

arrayLiteral
    : '[' Newline* (expression (Comma Newline* expression)* Comma? Newline*)? ']'
    ;

// Point { x: 1, y: 2 }   Savings { ..Account.new(id), rate: 0.02 }
// { "a": 1, "b": 2 }   ou {} (le type vient de l'annotation : map<string, int>)
mapLiteral
    : '{' Newline* (mapEntry (Comma Newline* mapEntry)* Comma? Newline*)? '}'
    ;

mapEntry: expression Colon expression;

structLiteral
    : Identifier typeArguments? '{' Newline*
      (structLiteralItem (Comma Newline* structLiteralItem)* Comma? Newline*)? '}'
    ;

structLiteralItem
    : Identifier Colon expression
    | Identifier                      // raccourci : Point { x, y }
    | DotDot expression
    ;

// if ... then ... (elseif ... then ...)* (else ...)? end
// Sans else, le if n'a pas de valeur ; avec une valeur attendue, else est exigé.
ifExpression
    : If expression Then block
      (ElseIf expression Then block)*
      (Else block)?
      End
    ;

// match expr (case motif (if garde)? then bloc)* (else bloc)? end
matchExpression
    : Match expression Newline+
      (matchCase)*
      (Else block)?
      End
    ;

matchCase: Case pattern (If expression)? Then block;

// motifs : littéral, variante d'enum (Color.Red), intervalle, liaison (x)
pattern
    : Minus? (IntLiteral | FloatLiteral) ((DotDot | DotDotEq) Minus? (IntLiteral | FloatLiteral))?
    | StringLiteral
    | True
    | False
    | None
    | Identifier (Dot Identifier)*
    ;

// fun(x: int) -> int ... end
functionExpression
    : Fun '(' parameterList? ')' (Arrow type)? block End
    ;

parameterList: parameter (Comma Newline* parameter)* Comma?;

parameter
    : Self
    | Identifier Colon type
    ;

// ====================================================================
// PARSER — Instructions et blocs (section 3)
// ====================================================================
// Une instruction par ligne, sans ';'. Les instructions d'un bloc sont
// séparées par des Newline ; la dernière peut être suivie directement du
// mot-clé fermant (if x then print(1) end tient sur une ligne).
//
// La valeur d'un bloc est celle de sa dernière instruction quand c'est
// une expression (fonctions, if, match) ; sinon le bloc n'a pas de valeur.
//
// Les conditions (if, elseif, while, case ... if) doivent être de type
// bool : c'est vérifié par le typage, pas par la grammaire.

block: Newline* (statement (Newline+ statement)* Newline*)?;

statement
    : variableDeclaration
    | returnStatement
    | whileStatement
    | forStatement
    | breakStatement
    | continueStatement
    | expression
    ;

// let x = 1   let x: int = 1   const PI = 3.14   let v: int? = none
// L'initialisation est obligatoire : pas de variable non initialisée.
variableDeclaration
    : (Let | Const) Identifier (Colon type)? Assign Newline* expression
    ;

// return   return expr   (sortie anticipée ; sinon la dernière expression est la valeur)
returnStatement: Return expression?;

whileStatement: While expression Do block End;
// for i in 0..10 step 2 do ... end   for x in items do ... end
// for i, x in items do ... end    for key, value in map do ... end
forStatement: For Identifier (Comma Identifier)? In expression (Step expression)? Do block End;

breakStatement: Break;

continueStatement: Continue;

// ====================================================================
// PARSER — Types (section 4)
// ====================================================================
// Types de base en minuscules : int, float, bool, string, array<T>
// (ce sont de simples identifiants, pas des mots-clés).
// Types définis par l'utilisateur : majuscule par convention.
//
//   int            float          bool           string
//   int?           array<int>     array<array<int>>
//   array<int>?    math.Point     Pair<int, string>
//   fun(int, int) -> int          (fun(int) -> int)?
//
// '?' rend un type nullable (none n'est permis que pour T?).
// Un seul '?' : int?? n'existe pas.

type
    : nullableType
    | functionType
    ;

nullableType: baseType Question?;

baseType
    : namedType
    | '(' type ')'
    ;

// math.Point : type exporté par un module importé sous alias
namedType: Identifier (Dot Identifier)* typeArguments?;

typeArguments: Lt type (Comma type)* Gt;

// le type de retour est facultatif (pas de valeur) ; fun(int) -> int?
// retourne int?, pas une fonction nullable : écrire (fun(int) -> int)?
functionType: Fun '(' (type (Comma type)*)? ')' (Arrow type)?;

// Paramètres de type avec bornes : <T>, <T: Add>, <T: Add + Eq, U>
// Utilisés par les fonctions, structs, traits (section 5).
typeParameters: Lt typeParameter (Comma typeParameter)* Gt;

typeParameter: Identifier (Colon bound (Plus bound)*)?;

// une borne est un trait, éventuellement générique : T: Iter<int>
bound: namedType;

// ====================================================================
// PARSER — Déclarations (section 5)
// ====================================================================
// Les déclarations n'existent qu'au niveau du fichier (pas dans un
// bloc). Une fonction locale s'écrit let f = fun(...) ... end.

topLevelItem
    : importDeclaration
    | exportDeclaration
    | functionDeclaration
    | structDeclaration
    | implDeclaration
    | traitDeclaration
    | enumDeclaration
    | statement
    ;

// --------------------------------------------------------------------
// Modules
// --------------------------------------------------------------------

// import add, sub as minus from "math.q"      import "math.q" as math
importDeclaration
    : Import importName (Comma importName)* From StringLiteral
    | Import StringLiteral As Identifier
    ;

importName: Identifier (As Identifier)?;

// export add, Point      (noms de déclarations du fichier)
exportDeclaration: Export Identifier (Comma Identifier)*;

// --------------------------------------------------------------------
// Fonctions
// --------------------------------------------------------------------

// fun add<T: Add>(a: T, b: T) -> T ... end
// La dernière expression du bloc est la valeur de retour.
functionDeclaration
    : Fun Identifier typeParameters? '(' parameterList? ')' (Arrow type)? block End
    ;

// --------------------------------------------------------------------
// struct
// --------------------------------------------------------------------

// struct Savings extends Account
//   public rate: float = 0.02
//   private static count: int = 0
// end
// Un seul parent. Un champ sans valeur par défaut doit être donné
// dans le littéral de struct.
structDeclaration
    : Struct Identifier typeParameters? (Extends namedType)? Newline
      (structField Newline+)*
      End
    ;

// visibilité facultative (private par défaut) ; static = champ du type
structField
    : visibility? Static? Identifier Colon type (Assign expression)?
    ;

visibility: Public | Private | Protected;

// --------------------------------------------------------------------
// impl
// --------------------------------------------------------------------

// impl Point ... end                 méthodes de Point
// impl Add for Point ... end         implémentation d'un trait
// impl<T> Box<T> ... end             type générique
// Une méthode avec 'self' en premier paramètre est une méthode d'instance ;
// sans 'self', elle est statique (Point.new(...)).
implDeclaration
    : Impl typeParameters? namedType (For namedType)? Newline
      (methodDeclaration Newline+)*
      End
    ;

// public fun dist(self) -> float ... end
// public override fun describe(self) -> string ... end
methodDeclaration
    : visibility? Override? Fun Identifier typeParameters?
      '(' parameterList? ')' (Arrow type)? block End
    ;

// --------------------------------------------------------------------
// trait
// --------------------------------------------------------------------

// trait Shape
//   fun area(self) -> float
// end
// Signatures seulement (pas de méthode par défaut, pas de super-trait).
traitDeclaration
    : Trait Identifier typeParameters? Newline
      (methodSignature Newline+)*
      End
    ;

methodSignature
    : Fun Identifier typeParameters? '(' parameterList? ')' (Arrow type)?
    ;

// --------------------------------------------------------------------
// enum
// --------------------------------------------------------------------

// enum Color
//   Red
//   Green
//   Blue
// end
// Variantes simples (constantes nommées, sans données). Accès : Color.Red
enumDeclaration
    : Enum Identifier Newline
      (Identifier Newline+)*
      End
    ;

// ====================================================================
// LEXER
// ====================================================================

// --------------------------------------------------------------------
// Mots-clés (doivent précéder Identifier)
// --------------------------------------------------------------------
// int, float, bool, string, array ne sont PAS des mots-clés : ce sont
// des identifiants (permet int.parse(s)).

Let: 'let';
Const: 'const';
Fun: 'fun';
Return: 'return';

If: 'if';
Then: 'then';
ElseIf: 'elseif';
Else: 'else';
End: 'end';

While: 'while';
For: 'for';
In: 'in';
Step: 'step';
Do: 'do';
Break: 'break';
Continue: 'continue';

Match: 'match';
Case: 'case';

Struct: 'struct';
Impl: 'impl';
Trait: 'trait';
Enum: 'enum';
Extends: 'extends';
Override: 'override';

Public: 'public';
Private: 'private';
Protected: 'protected';
Static: 'static';

Import: 'import';
Export: 'export';
From: 'from';
As: 'as';

And: 'and';
Or: 'or';
Not: 'not';
DivKw: 'div';
ModKw: 'mod';

True: 'true';
False: 'false';
None: 'none';
Self: 'self';
Super: 'super';

// --------------------------------------------------------------------
// Opérateurs et ponctuation
// --------------------------------------------------------------------
// Pas de '<<' ni '>>' : array<array<int>> se lexe sans ambiguïté.

Power: '**';
Plus: '+';
Minus: '-';
Star: '*';
Slash: '/';
Percent: '%';

EqEq: '==';
NotEq: '!=';
LtEq: '<=';
GtEq: '>=';
Lt: '<';
Gt: '>';

PlusAssign: '+=';
MinusAssign: '-=';
StarAssign: '*=';
SlashAssign: '/=';
PercentAssign: '%=';
IntDivAssign: 'div=';
ModAssign: 'mod=';
Assign: '=';

DotDotEq: '..=';
DotDot: '..';
Dot: '.';
Arrow: '->';
Question: '?';
Comma: ',';
Colon: ':';

LParen: '(';
RParen: ')';
LBracket: '[';
RBracket: ']';
LBrace: '{';
RBrace: '}';

// --------------------------------------------------------------------
// Littéraux
// --------------------------------------------------------------------
// Le '_' est autorisé entre deux chiffres : 1_000, 0xFF_FF, 0b0101_0101.
// Un float exige un chiffre après le point : 0..10 donne Int, '..', Int.

IntLiteral
    : '0' [xX] HexDigit ('_'? HexDigit)*
    | '0' [bB] [01] ('_'? [01])*
    | Digit ('_'? Digit)*
    ;

FloatLiteral
    : Digit ('_'? Digit)* '.' Digit ('_'? Digit)* Exponent?
    | Digit ('_'? Digit)* Exponent
    ;

// Chaîne sur une ou plusieurs lignes. Les {expr} d'interpolation restent
// dans le token : ils sont réanalysés par l'interpréteur (pas de '"' non
// échappé à l'intérieur d'un { }). Échappements : \n \t \r \0 \\ \" \{ \}
StringLiteral: '"' (Escape | ~["\\])* '"';

fragment Escape: '\\' [ntr0\\"{}];
fragment Digit: [0-9];
fragment HexDigit: [0-9a-fA-F];
fragment Exponent: [eE] [+-]? Digit ('_'? Digit)*;

// --------------------------------------------------------------------
// Identifiants (ASCII uniquement)
// --------------------------------------------------------------------

Identifier: [a-zA-Z_] [a-zA-Z_0-9]*;

// --------------------------------------------------------------------
// Commentaires et espaces
// --------------------------------------------------------------------
// '--' suivi de '(' ou '"' n'est jamais un commentaire de ligne.
// Pas d'imbrication. Les commentaires de doc vont sur le canal HIDDEN.

LineComment: '--' (~[("\r\n] ~[\r\n]*)? -> skip;
BlockComment: '--(' .*? '--)' -> skip;
DocComment: '--"' .*? '--"' -> channel(HIDDEN);

// Le retour à la ligne est significatif (fin d'instruction).
// Les règles de continuation (après opérateur, ',' ou '(' ouvert) seront
// dans le parseur, via Newline*.
Newline: ('\r'? '\n')+;

Whitespace: [ \t]+ -> skip;
