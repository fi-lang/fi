use parser::syntax_kind::*;

use crate::ast_node;

ast_node!(SourceFile, SOURCE_FILE);
ast_node!(Exports, EXPORTS);

ast_node!(Export {
    Name(ExportName, EXPORT_NAME),
    Module(ExportModule, EXPORT_MODULE),
    Group(ExportGroup, EXPORT_GROUP),
});

ast_node!(ExportGroupKind {
    All(ExportGroupAll, EXPORT_GROUP_ALL),
    Named(ExportGroupNamed, EXPORT_GROUP_NAMED),
});

ast_node!(Attr, ATTR);
ast_node!(AttrArgs, ATTR_ARGS);

ast_node!(AttrArg {
    Ident(AttrArgIdent, ATTR_ARG_IDENT),
    Call(AttrArgCall, ATTR_ARG_CALL),
    Equal(AttrArgEqual, ATTR_ARG_EQUAL),
    Literal(AttrArgLit, ATTR_ARG_LIT),
});

ast_node!(Item {
    Module(ItemModule, MODULE),
    Import(ItemImport, IMPORT),
    Fixity(ItemFixity, ITEM_FIXITY),
    Func(ItemFunc, ITEM_FUNC),
    Static(ItemStatic, ITEM_STATIC),
    Const(ItemConst, ITEM_CONST),
    Type(ItemType, ITEM_TYPE),
    Class(ItemClass, ITEM_CLASS),
    Member(ItemMember, ITEM_MEMBER),
});

ast_node!(OneFunc, ONE_FUNC);
ast_node!(OneStatic, ONE_STATIC);
ast_node!(OneConst, ONE_CONST);
ast_node!(OneType, ONE_TYPE);

ast_node!(ImportItems, IMPORT_ITEMS);
ast_node!(Ctor, ITEM_CTOR);

ast_node!(TypeVars, TYPE_VARS);
ast_node!(FunDep, FUN_DEP);

ast_node!(WhereClause, WHERE_CLAUSE);
ast_node!(Constraint, CONSTRAINT);
ast_node!(TypeVarKind, TYPE_VAR_KIND);

ast_node!(@ AssocItem {
    Func(ItemFunc, ITEM_FUNC),
    Static(ItemStatic, ITEM_STATIC),
});

ast_node!(Pat {
    Typed(PatTyped, PAT_TYPED),
    Wildcard(PatWildcard, PAT_WILDCARD),
    Infix(PatInfix, PAT_INFIX),
    App(PatApp, PAT_APP),
    Ctor(PatCtor, PAT_CTOR),
    Bind(PatBind, PAT_BIND),
    Lit(PatLit, PAT_LITERAL),
    Unit(PatUnit, PAT_UNIT),
    Parens(PatParens, PAT_PARENS),
    Record(PatRecord, PAT_RECORD),
});

ast_node!(Type {
    Hole(TypeHole, TYPE_HOLE),
    Infix(TypeInfix, TYPE_INFIX),
    Figure(TypeFigure, TYPE_FIGURE),
    Symbol(TypeSymbol, TYPE_SYMBOL),
    Unit(TypeUnit, TYPE_UNIT),
    App(TypeApp, TYPE_APP),
    Path(TypePath, TYPE_PATH),
    Rec(TypeRec, TYPE_REC),
    Row(TypeRow, TYPE_ROW),
    Parens(TypeParens, TYPE_PARENS),
    Forall(TypeForall, TYPE_FORALL),
    Where(TypeWhere, TYPE_WHERE),
});

ast_node!(RowField, ROW_FIELD);
ast_node!(RowTail, ROW_TAIL);

ast_node!(Stmt {
    Let(StmtLet, STMT_LET),
    Bind(StmtBind, STMT_BIND),
    Expr(StmtExpr, STMT_EXPR),
});

ast_node!(Expr {
    Typed(ExprTyped, EXPR_TYPED),
    Hole(ExprHole, EXPR_HOLE),
    Infix(ExprInfix, EXPR_INFIX),
    App(ExprApp, EXPR_APP),
    Field(ExprField, EXPR_FIELD),
    Method(ExprMethod, EXPR_METHOD),
    Path(ExprPath, EXPR_PATH),
    Lit(ExprLit, EXPR_LITERAL),
    Unit(ExprUnit, EXPR_UNIT),
    Parens(ExprParens, EXPR_PARENS),
    Record(ExprRecord, EXPR_RECORD),
    Array(ExprArray, EXPR_ARRAY),
    Do(ExprDo, EXPR_DO),
    Try(ExprTry, EXPR_TRY),
    Clos(ExprClos, EXPR_CLOS),
    If(ExprIf, EXPR_IF),
    Case(ExprCase, EXPR_CASE),
    Recur(ExprRecur, EXPR_RECUR),
    Return(ExprReturn, EXPR_RETURN),
});

ast_node!(Block, BLOCK);

ast_node!(CaseArm, CASE_ARM);
ast_node!(CaseGuard, CASE_GUARD);

ast_node!(CaseValue {
    Normal(CaseValueNormal, CASE_VALUE),
    Guarded(CaseValueGuarded, CASE_GUARDED),
});

ast_node!(Field {
    Normal(FieldNormal, FIELD_NORMAL),
    Pun(FieldPun, FIELD_PUN),
});

ast_node!(Path, PATH);
ast_node!(PathSegment, PATH_SEGMENT);

ast_node!(Name, NAME);
ast_node!(NameRef, NAME_REF);

ast_node!(Literal {
    Int(LitInt, LIT_INT),
    Float(LitFloat, LIT_FLOAT),
    Char(LitChar, LIT_CHAR),
    String(LitString, LIT_STRING),
});
