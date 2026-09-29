grammar qlang;

// ====================================================================
// qlang — grammaire ANTLR
// ====================================================================

program: statement* EOF;

// --------------------------------------------------------------------
// Statements
// --------------------------------------------------------------------

statement
    : variableDeclarationStatement
    | matchStatement
    | printStatement
    | ifStatement
    | whileStatement
    | forStatement
    | functionDeclarationStatement
    | breakStatement
    | continueStatement
    | returnStatement
    | includeStatement
    | structStatement
    | implStatement
    | expressionStatement
    ;

// dec/constante x (= expression | fonction ... fin)?
variableDeclarationStatement
    : (Let | Const) Identifier ('=' variableValue)?
    ;

variableValue: functionDeclarationStatement | expression;

// selon expr (cas expr alors bloc)* (sinon bloc)? fin
matchStatement
    : Match expression
      (Case expression Then block)*
      (Else block)?
      End
    ;

// ecrire expression
printStatement: Print expression;

// si expr alors bloc (sinonsi expr alors bloc)* (sinon bloc)? fin
ifStatement
    : If expression Then block
      (ElseIf expression Then block)*
      (Else block)?
      End
    ;

// tantque expr alors bloc fin
whileStatement: While expression Then block End;

// pour x de debut jusque fin (evol pas)? alors bloc fin
forStatement
    : For Identifier From expression Until expression (Step expression)? Then block End
    ;

// fonction nom? (params?) bloc fin — statement ou valeur (fonction anonyme)
functionDeclarationStatement
    : Function Identifier? '(' parameterList? ')' block End
    ;

parameterList: Identifier (',' Identifier)*;

breakStatement: Break;
continueStatement: Continue;
returnStatement: Return expression;

// inclure "chemin.q"
includeStatement: Include expression;

// structure Nom avec (visibilite champ (= valeur)?)* fin
structStatement
    : Structure Identifier With structField* End
    ;

structField: visibility Identifier ('=' expression)?;

// dans Nom implemente (methode)* fin
implStatement
    : In Identifier Implements methodDeclaration* End
    ;

// visibilite nom(params?) bloc fin
// (le premier paramètre "moi" désigne une méthode d'instance, sinon statique)
methodDeclaration
    : visibility Identifier '(' parameterList? ')' block End
    ;

visibility: Public | Hidden | Shared;

expressionStatement: expression;

block: statement*;

// --------------------------------------------------------------------
// Expressions (par précédence croissante)
// --------------------------------------------------------------------

expression: assignmentExpression;

// cible = valeur | cible (+=|-=) valeur   (+=/-= désucré en a = a op b)
assignmentExpression
    : logicalExpression ('=' assignmentExpression)?
    | logicalExpression CompoundAssign assignmentExpression
    ;

// et / ou
logicalExpression
    : equalityExpression (('et' | 'ou') equalityExpression)*
    ;

// == / !=
equalityExpression
    : relationalExpression (('==' | '!=') relationalExpression)*
    ;

// < <= > >=
relationalExpression
    : additiveExpression (('<' | '<=' | '>' | '>=') additiveExpression)*
    ;

// + -
additiveExpression
    : multiplicativeExpression (('+' | '-') multiplicativeExpression)*
    ;

// * / %
multiplicativeExpression
    : unaryExpression (('*' | '/' | '%') unaryExpression)*
    ;

// - non
unaryExpression
    : ('-' | 'non') unaryExpression
    | postfixExpression
    ;

// accès tableau [i] / [] (append), accès champ .x, appel de méthode .x(...)
postfixExpression
    : primaryOrCallExpression
      ( '[' expression? ']'
      | '.' Identifier ('(' argumentList? ')')?
      )*
    ;

// littéral de tableau, ou appel(s) sur une expression de base
primaryOrCallExpression
    : arrayLiteral
    | callExpression
    ;

arrayLiteral: '[' (expression (',' expression)*)? ']';

callExpression
    : readExpression ('(' argumentList? ')')*
    ;

argumentList: argument (',' argument)*;
argument: functionDeclarationStatement | expression;

// lire expression
readExpression
    : Read expression
    | primaryExpression
    ;

primaryExpression
    : Identifier
    | Null
    | Boolean
    | Number
    | String
    | '(' expression ')'
    ;

// --------------------------------------------------------------------
// Lexer — mots-clés (doivent précéder Identifier)
// --------------------------------------------------------------------

Let: 'dec';
Const: 'constante';
If: 'si';
Then: 'alors';
Else: 'sinon';
ElseIf: 'sinonsi';
End: 'fin';
While: 'tantque';
For: 'pour';
From: 'de';
Until: 'jusque';
Step: 'evol';
Return: 'retour';
Break: 'arreter';
Continue: 'continuer';
Null: 'rien';
Read: 'lire';
Print: 'ecrire';
Function: 'fonction';
Include: 'inclure';
Structure: 'structure';
With: 'avec';
In: 'dans';
Implements: 'implemente';
Public: 'publique';
Hidden: 'cacher';
Shared: 'partager';
Match: 'selon';
Case: 'cas';

Boolean: 'vrai' | 'faux';

// --------------------------------------------------------------------
// Lexer — opérateurs et ponctuation
// --------------------------------------------------------------------

CompoundAssign: [+-] '=';

// --------------------------------------------------------------------
// Lexer — littéraux et identifiants
// --------------------------------------------------------------------

Number: [0-9]+ ('.' [0-9]+)?;

String
    : '"' (EscapeSequence | ~["\\])* '"'
    ;

fragment EscapeSequence: '\\' . ;

Identifier: [a-zA-Z_][a-zA-Z_0-9]*;

// --------------------------------------------------------------------
// Lexer — commentaires et espaces
// --------------------------------------------------------------------

// `rem` commente jusqu'à la fin de la ligne
Comment: 'rem' ~[\r\n]* -> skip;

Whitespace: [ \t\r\n]+ -> skip;
