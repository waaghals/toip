use nom::branch::alt;
use nom::bytes::complete::{tag, take_until, take_while1};
use nom::character::complete::{char, multispace0, one_of};
use nom::combinator::{map, recognize};
use nom::IResult;
use nom::multi::many1;
use nom::sequence::{delimited, preceded};
use nom::error::ParseError;
use crate::parser::CommandToken::Caret;
use std::iter::once;
use nom::number::complete::be_u8;

#[derive(Debug, Eq, PartialEq)]
enum CommandToken<'a> {
    Bare(&'a str),
    EnvVariable(&'a str),
    Tilde,
    ArgumentIndex(u8),
    ArgumentArray,
    Caret,
    SubShell(Shell<'a>),
}

struct CommandLine<'a>(Vec<CommandToken<'a>>);

// One more commands fed into each other
struct PipedCommands<'a>(Vec<CommandLine<'a>>);

struct Shell<'a>(Vec<ShellToken<'a>>);

#[derive(Debug, Eq, PartialEq)]
enum ShellToken<'a> {
    Commands(PipedCommands<'a>),
    IfAnd,
    IfOr,
}

//#[derive(Debug, Eq, PartialEq)]
//struct Token<'a>(Vec<TokenPart<'a>>);

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

fn var_char(ch: char) -> bool {
    match ch {
        'a'..='z' => true,
        'A'..='Z' => true,
        '0'..='9' => true,
        '_' => true,
        _ => false,
    }
}

fn bare(input: &str) -> IResult<&str, CommandToken> {
    map(take_while1(token_char), |s| CommandToken::Bare(s))(input)
}

fn quoted(input: &str) -> IResult<&str, CommandToken> {
    map(
        delimited(
            tag("\""),
            take_until("\""),
            tag("\""),
        ),
        |s| CommandToken::Bare(s),
    )(input)
}

fn env_var(input: &str) -> IResult<&str, CommandToken> {
    map(
        delimited(
            tag("${"),
            take_while1(var_char),
            char('}'),
        ),
        |name| CommandToken::EnvVariable(name),
    )(input)
}

fn tilde(input: &str) -> IResult<&str, CommandToken> {
    map(char('~'), |_| CommandToken::Tilde)(input)
}

fn caret(input: &str) -> IResult<&str, CommandToken> {
    map(char('^'), |_| CommandToken::Caret)(input)
}

fn argument(input: &str) -> IResult<&str, CommandToken> {
    map(
        preceded(
            char('$'),
            one_of("0123456789"),
        ),
        |index| CommandToken::ArgumentIndex(index.to_digit(10).unwrap() as u8),
    )(input)
}

fn sub_shell(input: &str) -> IResult<&str, CommandToken> {
    map(
        delimited(
            tag("$("),
            command_line, //TODO shell
            tag(")"),
        ),
        |command| CommandToken::SubShell(command),
    )
}

fn command_line(input: &str) -> IResult<&str, CommandLine> {
    map(
        many1(
            delimited(
                multispace0,
                alt((tilde, argument, env_var, bare, quoted)),
                multispace0),
        ),
        |vec| CommandLine(vec),
    )(input)
}


#[cfg(test)]
mod tests {
    use super::*;
    use nom::error::ErrorKind::TakeWhile1;
    use nom::Err::Error;

    #[test]
    fn test_bare() {
        assert_eq!(bare(""), Err(Error(("", TakeWhile1))));
        assert_eq!(bare("test>"), Ok((">", CommandToken::Bare("test"))));
        assert_eq!(bare("test💩"), Ok(("💩", CommandToken::Bare("test"))));
    }

    #[test]
    fn test_quoted() {
        assert_eq!(quoted("\"test\""), Ok(("", CommandToken::Bare("test"))));
    }

    #[test]
    fn test_env_var() {
        assert_eq!(env_var("${test}"), Ok(("", CommandToken::EnvVariable("test"))));
    }

    #[test]
    fn test_tilde() {
        assert_eq!(tilde("~"), Ok(("", CommandToken::Tilde)));
    }

    #[test]
    fn test_caret() {
        assert_eq!(caret("^"), Ok(("", CommandToken::Caret)));
    }

    #[test]
    fn test_argument() {
        assert_eq!(argument("$0"), Ok(("", CommandToken::ArgumentIndex(0))));
        assert_eq!(argument("$1"), Ok(("", CommandToken::ArgumentIndex(1))));
        assert_eq!(argument("$10"), Ok(("0", CommandToken::ArgumentIndex(1))));
    }

    #[test]
    fn test_command() {
        let expected_tokens = vec![
            CommandToken::Bare("cargo"),
            CommandToken::Bare("run"),
            CommandToken::ArgumentIndex(1),
            CommandToken::Tilde,
            CommandToken::Bare("/"),
            CommandToken::EnvVariable("SOME_PATH"),
            CommandToken::Bare("-v"),
            CommandToken::Caret,
            CommandToken::ArgumentIndex(2),
        ];
        assert_eq!(tokens("cargo run $1 ~/${SOME_PATH} -v ^ $2"), Ok(("", expected_tokens)));
    }
}