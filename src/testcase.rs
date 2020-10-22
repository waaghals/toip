use std::iter::once;
use std::process::Command;

use nom::branch::alt;
use nom::bytes::complete::{tag, take_until, take_while1, take_while, take};
use nom::character::complete::{char, multispace0, one_of};
use nom::combinator::{map, recognize, value};
use nom::error::ParseError;
use nom::IResult;
use nom::multi::{many1, separated_nonempty_list, many0};
use nom::number::complete::be_u8;
use nom::sequence::{delimited, preceded, tuple, terminated};

use nom::character::is_alphabetic;

fn token_char(ch: char) -> bool {
    if ch.len_utf8() > 1 {
        return false;
    }
    match ch {
        '\x00'..='\x20' => false,
        '\x7f' | '"' | '\'' | '>' | '<' | '|' | ';' | '{' | '}' | '$' => false,
        _ => true,
    }
}

#[derive(Debug, Eq, PartialEq)]
enum Token<'a> {
    Other(&'a str),
    SubShell(Shell<'a>),
}

#[derive(Debug, Eq, PartialEq)]
struct Shell<'a>(Vec<Token<'a>>);

fn other(input: &str) -> IResult<&str, Token> {
    map(take_while1(token_char), |s| Token::Other(s))(input)
}

fn shell(input: &str) -> IResult<&str, Shell> {
    map(
        many1(
            alt((
                other,
                sub_shell
            )),
        ),
        |vec| Shell(vec),
    )(input)
}

fn sub_shell(input: &str) -> IResult<&str, Token> {
    let (remaining, matched) = delimited(
        tag("$("),
        take_until(")"),
        tag(")"),
    )(input)?;
    println!("matced: {}", matched);
    println!("remaining: {}", remaining);
    let (remaining, shell) = shell(matched)?;

    Ok((remaining, Token::SubShell(shell)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test() {
        println!("{:#?}", shell("$(b $(c))"));
    }
}