use crate::compiler::{
    syntax::Syntax,
    token::Token,
    ast::AST,
};

use crate::common::{
    span::{Span, Spanned},
    local::Local,
};

// This is a recursive descent parser that builds the AST
// TODO: the 'vacuum' seems kind of cheap.

// some sort of recursive descent parser, I guess
type Tokens<'a> = &'a [Spanned<Token>];
type Bite<'a>   = (Spanned<AST>, Tokens<'a>);
type Rule   = Box<dyn Fn(Tokens) -> Result<Bite, Syntax>>;

pub fn parse<'a>(tokens: Vec<Spanned<Token>>) -> Result<Spanned<AST>, Syntax> {
    // parse the file
    // slices are easier to work with
    match block(&tokens) {
        (ast, Some(syntax), tokens) => { Err(syntax) },
        (ast, None, tokens) => if vaccum(tokens, Token::Sep).is_empty()
            { Ok(ast) } else { panic!("Did not consume all tokens") },
    }
}

// cookie-monster's helper functions

/// Consumes all next tokens that match.
/// For example, `[Sep, Sep, Sep, Number(...), Sep]`
/// when passed to `vaccum(..., Sep)`
/// would become `[Number(...), Sep]`.
/// Each parser rule is responsible for vaccuming its input.
fn vaccum(tokens: Tokens, token: Token) -> Tokens {
    // vaccums all leading tokens that match token
    let mut remaining = tokens;

    while !remaining.is_empty() {
        let t = &remaining[0].item;
        if t != &token { break; }
        remaining = &remaining[1..];
    }

    return remaining;
}

/// Expects an exact token to be next in a stream.
/// For example, `consume(stream, Bracket)` expects the next item in stream to be a `Bracket`.
fn consume(tokens: Tokens, token: Token) -> Result<Tokens, Syntax> {
    let t = match tokens.iter().next() {
        Some(t) => t,
        None => return Err(Syntax::error(
            "Unexpected EOF while parsing",
            Span::empty()
        )),
    };

    if t.item != token {
        return Err(Syntax::error(
            &format!(
                "Expected {}, found {} ({:?})",
                token,
                t.item,
                t.span.contents(),
            ),
            t.span.clone()
        ));
    }

    return Result::Ok(&tokens[1..]);
}

/// Given a list of parsing rules and a token stream,
/// This function returns the first rule result that successfully parses the token stream.
/// Think of 'or' for parser-combinators.
fn first(tokens: Tokens, rules: Vec<Rule>) -> Result<Bite, Syntax> {
    let mut worst: Option<Syntax> = None;

    println!("---");

    for rule in rules {
        println!("entering...");
        match rule(tokens) {
            Ok((ast, r)) => {
                println!("exiting matched: -> {}", ast.span);
                return Ok((ast, r));
            }
            Err(e) => {
                if let Some(ref p) = worst {
                    // if this error starts the latest and is the longest
                    if e.span.offset > p.span.offset
                       || (e.span.offset == p.span.offset
                          && e.span.end() > p.span.end())  {
                        println!("escalated to: -> {}", e);
                        worst = Some(e)
                    } else {
                        println!("no escalation");
                    }
                } else {
                    println!("worst error is: -> {}", e);
                    worst = Some(e);
                }
            }
        }
        println!("exiting...");
    }

    println!("all rules checked");

    // if nothing matched, return the first potential error
    if let Some(e) = worst {
        println!("returning error: -> {}", e);
        return Err(e);
    }

    println!("no matches!");

    match tokens.iter().next() {
        Some(t) => Err(Syntax::error("Unexpected construct", t.span.clone())),
        None    => Err(Syntax::error("Unexpected EOF while parsing", Span::empty())),
    }
}

// fn parse_op(tokens: Tokens, left: Rule, op: Token, right:Rule) -> Result<'e, (Spanned<'s, AST<'s, 'i>>, Tokens)> {
//     unimplemented!()
// }

/// Matches a literal block, i.e. a list of expressions seperated by separators.
/// Note that block expressions `{ e 1, ..., e n }` are blocks surrounded by `{}`.
fn block(tokens: Tokens) -> (Spanned<AST>, Option<Syntax>, Tokens) {
    let mut expressions = vec![];
    let mut annotations = vec![];
    let mut remaining   = vaccum(tokens, Token::Sep);
    let mut error       = None;

    while !remaining.is_empty() {
        match call(remaining) {
            Result::Ok((e, r)) => {
                annotations.push(e.span.clone());
                expressions.push(e);
                remaining = r;
            },
            Err(e) => {
                error = Some(e);
                break;
            },
        }

        // TODO: implement one-or-more
        // expect at least one separator between statements
        remaining = vaccum(remaining, Token::Sep);
    }

    // TODO: is this true? an empty program is should be valid
    // what does it make sense for an empty block to return?
    // empty blocks don't make any sense - use unit
    if annotations.is_empty() {
        panic!("annotations were empty");
        // return Err(Syntax::error("Block can't be empty, use Unit '()' instead", Span::empty()))
    }

    let ast = Spanned::new(AST::block(expressions), Span::join(annotations));
    return (ast, error, remaining);
}

/// Matches a function call, i.e. `f x y z`.
/// Function calls are left binding,
/// so the above is parsed as `((f x) y) z`.
fn call(tokens: Tokens) -> Result<Bite, Syntax> {
    println!("-- try parse call");
    // try to eat an new expression
    // if it's successfull, nest like so:
    // previous = Call(previous, new)
    // empty    => error
    // single   => expression
    // multiple => call
    let (mut previous, mut remaining) = expr(vaccum(tokens, Token::Sep))?;

    while !remaining.is_empty() {
        match expr(remaining) {
            Result::Ok((arg, r)) => {
                remaining = r;
                let span = Span::combine(&previous.span, &arg.span);
                previous = Spanned::new(AST::call(previous, arg), span);
            },
            _ => break,
        }
    }

    return Result::Ok((previous, remaining));
}

/// Matches an expression, or more tightly binding expressions.
fn expr(tokens: Tokens) -> Result<Bite, Syntax> {
    println!("-- try parse expr");
    let rules: Vec<Rule> = vec![
        Box::new(|s| expr_block(s)),
        Box::new(|s| expr_call(s)),
        Box::new(|s| op(s)),
        Box::new(|s| literal(s)),
    ];

    return first(tokens, rules);
}

/// Matches a literal block, `{ expression 1; ...; expression n }`.
fn expr_block(tokens: Tokens) -> Result<Bite, Syntax> {
    println!("-- try parse expr block");
    // match the opening bracket
    let start = consume(tokens, Token::OpenBracket)?;

    // try to parse as much as possible as a block body
    let (ast, error, remaining) = block(start);
    println!("-- parsed block body...");

    // when we can't anymore, match the closing bracket
    return match consume(remaining, Token::CloseBracket) {
        // if the closing bracket is matched, ignore the earlier error
        // because we break on errors when parsing an expression AST, it's still valid
        Ok(tokens) => Ok((ast, tokens)),
        Err(e) => {
            println!("-- but there was an error: no closing bracket!");
            // pass earlier error if one occured
            if let Some(syntax) = error {
                println!("-- this might've been because of an earlier error");
                Err(syntax)
            } else {
                println!("-- let's let them know!");
                Err(e)
            }
        },
    };
}

fn expr_call(tokens: Tokens) -> Result<Bite, Syntax> {
    println!("-- try parse expr call");
    let start      = consume(tokens, Token::OpenParen)?;
    let (ast, end) = call(start)?;
    let remaining  = consume(end, Token::CloseParen)?;

    return Result::Ok((ast, remaining));
}

fn op(tokens: Tokens) -> Result<Bite, Syntax> {
    assign(tokens)
}

/// Matches an assignment or more tightly binding expressions.
fn assign(tokens: Tokens) -> Result<Bite, Syntax> {
    println!("-- try parse assign");
    let rules: Vec<Rule> = vec![
        Box::new(|s| assign_assign(s)),
        Box::new(|s| lambda(s)),
    ];

    return first(tokens, rules);
}

// TODO: implement parse_op and rewrite lambda / assign

/// Matches an actual assignment, `pattern = expression`.
fn assign_assign(tokens: Tokens) -> Result<Bite, Syntax> {
    println!("-- try parse assign assign");
    // TODO: pattern matching support!
    // get symbol being assigned too
    let (next, mut remaining) = literal(tokens)?;
    let s = match next {
        // Destructure restucture
        spanned @ Spanned { item: AST::Symbol, span: _ } => spanned,
        other => return Err(Syntax::error("Expected symbol for assignment", other.span)),
    };

    // eat the = sign
    remaining = consume(remaining, Token::Assign)?;
    let (e, remaining) = call(remaining)?;
    let combined       = Span::combine(&s.span, &e.span);
    Result::Ok((Spanned::new(AST::assign(s, e), combined), remaining))
}

/// Matches a function, `pattern -> expression`.
fn lambda(tokens: Tokens) -> Result<Bite, Syntax> {
    println!("-- try parse lambda");
    // get symbol acting as arg to function
    let (next, mut remaining) = literal(tokens)?;
    let s = match next {
        spanned @ Spanned { item: AST::Symbol, span: _ } => spanned,
        other => return Err(Syntax::error("Expected symbol for function paramater", other.span)),
    };

    // eat the '->'
    remaining = consume(remaining, Token::Lambda)?;
    let (e, remaining) = call(remaining)?;
    let combined       = Span::combine(&s.span, &e.span);
    Result::Ok((Spanned::new(AST::lambda(s, e), combined), remaining))
}

/// Matches some literal data, such as a String or a Number.
fn literal(tokens: Tokens) -> Result<Bite, Syntax> {
    println!("-- try parse literal");
    if let Some(Spanned { item: token, span }) = tokens.iter().next() {
        Result::Ok((Spanned::new(
            match token {
                // TODO: pass the span
                Token::Symbol     => AST::symbol(),
                Token::Number(n)  => AST::data(n.clone()),
                Token::String(s)  => AST::data(s.clone()),
                Token::Boolean(b) => AST::data(b.clone()),
                _ => return Err(Syntax::error("Unexpected token", span.clone())),
            },
            span.clone()
        ), &tokens[1..]))
    } else {
        Err(Syntax::error("Unexpected EOF while parsing", Span::empty()))
    }
}

// TODO: ASTs can get really big, really fast - have tests in external file?
// #[cfg(test)]
// mod test {
//     use crate::pipeline::source::Source;
//     use crate::compiler::lex::lex;
//     use super::*;
//
//     #[test]
//     fn assignment() {
//         // who knew so little could mean so much?
//         // forget verbose, we should all write ~~lisp~~ ast
//         let source = Source::source("heck = false; naw = heck");
//
//         // oof, I wrote this out by hand
//         let result = AST::new(
//             Node::block(vec![
//                 AST::new(
//                     Node::assign(
//                         AST::new(Node::symbol(Local::new("heck".to_string())), Span::new(&source, 0, 4)),
//                         AST::new(Node::data(Data::Boolean(false)), Span::new(&source, 7, 5)),
//                     ),
//                     Span::new(&source, 0, 12),
//                 ),
//                 AST::new(
//                     Node::assign(
//                         AST::new(Node::Symbol(Local::new("naw".to_string())), Span::new(&source, 14, 3)),
//                         AST::new(Node::Symbol(Local::new("heck".to_string())), Span::new(&source, 20, 4)),
//                     ),
//                     Span::new(&source, 14, 10),
//                 ),
//             ]),
//             Span::new(&source, 0, 24),
//         );
//
//         assert_eq!(parse(lex(source).unwrap()), Result::Ok(result));
//     }
//
//     #[test]
//     fn failure() {
//         let source = Source::source("\n hello9 = {; ");
//
//         // assert_eq!(parse(lex(source).unwrap()), Err(CompilerError()));
//         // TODO: determing exactly which error is thrown
//         panic!();
//     }
//
//     #[test]
//     fn block() {
//         // TODO: Put this bad-boy somewhere else.
//         // maybe just have one test file and a huge hand-verified ast
//         let source = Source::source("x = true\n{\n\ty = {x; true; false}\n\tz = false\n}");
//         let parsed = parse(lex(source).unwrap());
//         let result = Result::Ok(
//             AST::new(
//                 Node::block(vec![
//                     AST::new(
//                         Node::assign(
//                             AST::new(Node::symbol(Local::new("x".to_string())), Span::new(&source, 0, 1)),
//                             AST::new(Node::data(Data::Boolean(true)),           Span::new(&source, 4, 4)),
//                         ),
//                         Span::new(&source, 0, 8)
//                     ),
//                     AST::new(Node::block(
//                         vec![
//                             AST::new(
//                                 Node::assign(
//                                     AST::new(Node::symbol(Local::new("y".to_string())), Span::new(&source, 12, 1)),
//                                     AST::new(
//                                         Node::block(vec![
//                                             AST::new(Node::symbol(Local::new("x".to_string())), Span::new(&source, 17, 1)),
//                                             AST::new(Node::data(Data::Boolean(true)),           Span::new(&source, 20, 4)),
//                                             AST::new(Node::data(Data::Boolean(false)),          Span::new(&source, 26, 5)),
//                                         ]),
//                                         Span::new(&source, 17, 14),
//                                     )
//                                 ),
//                                 Span::new(&source, 12, 19),
//                             ),
//                             AST::new(
//                                 Node::assign(
//                                     AST::new(Node::symbol(Local::new("z".to_string())),Span::new(&source, 34, 1)),
//                                     AST::new(Node::data(Data::Boolean(false)), Span::new(&source, 38, 5)),
//                                 ),
//                                 Span::new(&source, 34, 9),
//                             ),
//                         ]),
//                         Span::new(&source, 12, 31),
//                     ),
//                 ]),
//                 Span::new(&source, 0, 43),
//             ),
//         );
//         assert_eq!(parsed, result);
//     }
//
//     #[test]
//     fn number() {
//         let source = Source::source("number = { true; 0.0 }");
//         let parsed = parse(lex(source).unwrap());
//         let result = Result::Ok(
//             AST::new(
//                 Node::block(vec![
//                     AST::new(
//                         Node::assign(
//                             AST::new(Node::symbol(Local::new("number".to_string())), Span::new(&source, 0, 6)),
//                             AST::new(
//                                 Node::block(vec![
//                                     AST::new(Node::data(Data::Boolean(true)), Span::new(&source, 11, 4)),
//                                     AST::new(Node::data(Data::Real(0.0)), Span::new(&source, 17, 3)),
//                                 ]),
//                                 Span::new(&source, 11, 9),
//                             ),
//                         ),
//                         Span::new(&source, 0, 20),
//                     )
//                 ]),
//                 Span::new(&source, 0, 20),
//             ),
//         );
//
//         assert_eq!(parsed, result);
//     }
//
//     #[test]
//     fn functions() {
//         let source = Source::source("applyzero = fun -> arg -> fun arg 0.0");
//         let parsed = parse(lex(source).unwrap());
//         let result = Result::Ok(
//             AST::new(
//                 Node::block(vec![
//                     AST::new(
//                         Node::assign(
//                             AST::new(Node::symbol(Local::new("applyzero".to_string())), Span::new(&source, 0, 9)),
//                             AST::new(
//                                 Node::lambda(
//                                     AST::new(Node::symbol(Local::new("fun".to_string())), Span::new(&source, 12, 3)),
//                                     AST::new(Node::lambda(
//                                         AST::new(Node::symbol(Local::new("arg".to_string())),  Span::new(&source, 19, 3)),
//                                         AST::new(
//                                             Node::call(
//                                                 AST::new(
//                                                     Node::call(
//                                                         AST::new(Node::symbol(Local::new("fun".to_string())), Span::new(&source, 26, 3)),
//                                                         AST::new(Node::symbol(Local::new("arg".to_string())), Span::new(&source, 30, 3)),
//                                                     ),
//                                                     Span::new(&source, 26, 7),
//                                                 ),
//                                                 AST::new(Node::data(Data::Real(0.0)), Span::new(&source, 34, 3)),
//                                             ),
//                                             Span::new(&source, 26, 11)
//                                         )
//                                     ),
//                                     Span::new(&source, 19, 18),
//                                 ),
//                             ),
//                             Span::new(&source, 12, 25),
//                         ),
//                     ),
//                     Span::new(&source, 0, 37),
//                 )]),
//                 Span::new(&source, 0, 37),
//             ),
//         );
//
//         assert_eq!(parsed, result);
//     }
//
//     #[test]
//     fn calling() {
//         let source = Source::source("bink (bonk 0.0)");
//         let parsed = parse(lex(source).unwrap());
//
//         let result = Result::Ok(
//             AST::new(
//                 Node::block(vec![
//                     AST::new(
//                         Node::call (
//                             AST::new(Node::symbol(Local::new("bink".to_string())), Span::new(&source, 0, 4)),
//                             AST::new(
//                                 Node::call(
//                                     AST::new(Node::symbol(Local::new("bonk".to_string())), Span::new(&source, 6, 4)),
//                                     AST::new(Node::data(Data::Real(0.0)), Span::new(&source, 11, 3)),
//                                 ),
//                                 Span::new(&source, 6, 8),
//                             ),
//                         ),
//                         Span::new(&source, 0, 14)
//                     ),
//                 ]),
//                 Span::new(&source, 0, 14),
//             ),
//         );
//         assert_eq!(parsed, result);
//     }
// }
