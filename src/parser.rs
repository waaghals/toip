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

use crate::parser::ProcessToken::Caret;
use nom::character::is_alphabetic;

#[derive(Debug, Eq, PartialEq)]
enum ProcessToken<'a> {
    Bare(&'a str),
    EnvVariable(&'a str),
    Tilde,
    ArgumentIndex(u8),
    ArgumentArray,
    Caret,
    SubShell(Shell<'a>),
}

// Single command with args and substitution
#[derive(Debug, Eq, PartialEq)]
struct Process<'a>(Vec<ProcessToken<'a>>);

// One more commands fed into each other
#[derive(Debug, Eq, PartialEq)]
struct ProcessGroup<'a>(Vec<Process<'a>>);

//struct AndCommands<'a>(Vec<PipedCommands<'a>>);
//struct OrCommands<'a>(Vec<PipedCommands<'a>>);
#[derive(Debug, Eq, PartialEq)]
struct Shell<'a>(Vec<ShellToken<'a>>);

#[derive(Debug, Eq, PartialEq)]
enum ShellToken<'a> {
    Process(ProcessGroup<'a>),
    And,
    Or,
}

//#[derive(Debug, Eq, PartialEq)]
//struct Token<'a>(Vec<TokenPart<'a>>);

// https://www.gnu.org/software/bash/manual/html_node/Definitions.html


fn token_char(ch: char) -> bool {
    if ch.len_utf8() > 1 {
        return false;
    }
    match ch {
        '\x00'..='\x20' => false,
        '\x7f' | '"' | '\'' | '>' | '<' | '|' | ';' | '{' | '}' | '$'  => false,
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

fn bare(input: &str) -> IResult<&str, ProcessToken> {
    map(take_while1(token_char), |s| ProcessToken::Bare(s))(input)
}

fn quoted(input: &str) -> IResult<&str, ProcessToken> {
    map(
        delimited(
            tag("\""),
            take_until("\""),
            tag("\""),
        ),
        |s| ProcessToken::Bare(s),
    )(input)
}

fn env_var(input: &str) -> IResult<&str, ProcessToken> {
    map(
        delimited(
            tag("${"),
            take_while1(var_char),
            char('}'),
        ),
        |name| ProcessToken::EnvVariable(name),
    )(input)
}

fn tilde(input: &str) -> IResult<&str, ProcessToken> {
    map(char('~'), |_| ProcessToken::Tilde)(input)
}

fn caret(input: &str) -> IResult<&str, ProcessToken> {
    map(char('^'), |_| ProcessToken::Caret)(input)
}

fn argument(input: &str) -> IResult<&str, ProcessToken> {
    map(
        preceded(
            char('$'),
            one_of("0123456789"),
        ),
        |index| ProcessToken::ArgumentIndex(index.to_digit(10).unwrap() as u8),
    )(input)
}

//fn ws<'a, O, E: ParseError<&'a str>, F: Parser<&'a str, O, E>>(f: F) -> impl Parser<&'a str, O, E> {
//    delimited(multispace0, f, multispace0)
//}

fn sub_shell(input: &str) -> IResult<&str, ProcessToken> {
    let (remaining, matched) = delimited(
        tag("$("),
        take_until(")"),
        tag(")"),
    )(input)?;
    println!("matced: {}", matched);
    println!("remaining: {}", remaining);
    let (remaining, shell) = tag("echo test")(matched)?;
//    let (remaining, shell) = shell(matched)?;


    Ok((remaining, ProcessToken::SubShell(
        Shell(vec![
            ShellToken::Process(
                ProcessGroup(vec![
                    Process(vec![
                        ProcessToken::Bare(shell)
                    ])
                ])
            )
        ])
    )))
}

fn process(input: &str) -> IResult<&str, Process> {
    map(
        many1(
            delimited(
                multispace0,
                alt((sub_shell, tilde, caret, argument, env_var, bare, quoted)),
                multispace0),
        ),
        |vec| Process(vec),
    )(input)
}

fn process_group(input: &str) -> IResult<&str, ProcessGroup> {
    map(
        separated_nonempty_list(
            delimited(
                multispace0,
                char('|'),
                multispace0,
            ),
            process,
        ),
        |pg| ProcessGroup(pg),
    )(input)
}

fn shell_and(input: &str) -> IResult<&str, ShellToken> {
    map(tag("&&"), |_| ShellToken::And)(input)
}

fn shell_or(input: &str) -> IResult<&str, ShellToken> {
    map(tag("||"), |_| ShellToken::Or)(input)
}

fn shell(input: &str) -> IResult<&str, Shell> {
    map(
        many1(
//            delimited(
//                multispace0,
alt(
    (
        shell_and,
        shell_or,
        map(
            process_group,
            |pg| ShellToken::Process(pg),
        )
    )),
//                multispace0),
        ),
        |vec| Shell(vec),
    )(input)
}

fn take_until_and_consume<T, I, E>(tag: T) -> impl Fn(I) -> nom::IResult<I, I, E>
    where
        E: nom::error::ParseError<I>,
        I: nom::InputTake
        + nom::FindSubstring<T>
        + nom::Slice<std::ops::RangeFrom<usize>>
        + nom::InputIter<Item=u8>
        + nom::InputLength,
        T: nom::InputLength + Clone,
{
    use nom::bytes::streaming::take;
    use nom::bytes::streaming::take_until;
    use nom::sequence::terminated;

    move |input| terminated(take_until(tag.clone()), take(tag.input_len()))(input)
}

#[cfg(test)]
mod tests {
    use nom::Err::Error;
    use nom::error::ErrorKind::TakeWhile1;

    use super::*;

    #[test]
    fn test_bare() {
        assert_eq!(bare(""), Err(Error(("", TakeWhile1))));
        assert_eq!(bare("test>"), Ok((">", ProcessToken::Bare("test"))));
        assert_eq!(bare("test💩"), Ok(("💩", ProcessToken::Bare("test"))));
    }

    #[test]
    fn test_quoted() {
        assert_eq!(quoted("\"test\""), Ok(("", ProcessToken::Bare("test"))));
    }

    #[test]
    fn test_env_var() {
        assert_eq!(env_var("${test}"), Ok(("", ProcessToken::EnvVariable("test"))));
    }

    #[test]
    fn test_tilde() {
        assert_eq!(tilde("~"), Ok(("", ProcessToken::Tilde)));
    }

    #[test]
    fn test_caret() {
        assert_eq!(caret("^"), Ok(("", ProcessToken::Caret)));
    }

    #[test]
    fn test_argument() {
        assert_eq!(argument("$0"), Ok(("", ProcessToken::ArgumentIndex(0))));
        assert_eq!(argument("$1"), Ok(("", ProcessToken::ArgumentIndex(1))));
        assert_eq!(argument("$10"), Ok(("0", ProcessToken::ArgumentIndex(1))));
    }

    #[test]
    fn test_command_line() {
        let expected_tokens = Process(vec![
            ProcessToken::Bare("cargo"),
            ProcessToken::Bare("run"),
            ProcessToken::ArgumentIndex(1),
            ProcessToken::Tilde,
            ProcessToken::Bare("/"),
            ProcessToken::EnvVariable("SOME_PATH"),
            ProcessToken::Bare("-v"),
            ProcessToken::Caret,
            ProcessToken::ArgumentIndex(2),
        ]);
        assert_eq!(process("cargo run $1 ~/${SOME_PATH} -v ^ $2"), Ok(("", expected_tokens)));
    }

    #[test]
    fn test_command_pipe() {
        let command_line_a = Process(vec![ProcessToken::Bare("a")]);
        let command_line_b = Process(vec![ProcessToken::Bare("b")]);
        let command_line_c = Process(vec![ProcessToken::Bare("c")]);

        assert_eq!(process_group("a | b | c"), Ok(("", ProcessGroup(vec![command_line_a, command_line_b, command_line_c]))));
    }

    #[test]
    fn test_shell_and() {
        assert_eq!(shell_and("&&"), Ok(("", ShellToken::And)));
    }

    #[test]
    fn test_shell_or() {
        assert_eq!(shell_or("||"), Ok(("", ShellToken::Or)));
    }

    #[test]
    fn test_shell() {
        let create_pipe = |ch| ProcessGroup(vec![Process(vec![ProcessToken::Bare(ch)])]);
        assert_eq!(
            shell("a && b || c || d && e"),
            Ok((
                "",
                Shell(vec![
                    ShellToken::Process(create_pipe("a")),
                    ShellToken::And,
                    ShellToken::Process(create_pipe("b")),
                    ShellToken::Or,
                    ShellToken::Process(create_pipe("c")),
                    ShellToken::Or,
                    ShellToken::Process(create_pipe("d")),
                    ShellToken::And,
                    ShellToken::Process(create_pipe("e"))
                ])
            ))
        );
    }

    #[test]
    fn test_sub_shell() {
//        println!("{:#?}", sub_shell("$(a|"));
//        println!("{:#?}", sub_shell("$(a)"));
        println!("{:#?}", sub_shell("$($(echo test))"));
//        println!("{:#?}", sub_shell("$(a && b ${B} && e $(nested subshell ${NESTED}) || something)"));
    }

    #[test]
    fn test_all() {
        println!("{:#?}", shell("echo ${TEST} || c ~/test || d $(a && b ${B} && e) && echo \"done\""));
//        let create_pipe = |ch| ProcessGroup(vec![Process(vec![ProcessToken::Bare(ch)])]);
//        assert_eq!(
//            shell("echo $(a && b || c || d && e) && echo \"done\""),
//            Ok((
//                "",
//                Shell(vec![
//                    ShellToken::Process(
//                        ProcessGroup(vec![
//                            Process(vec![
//                                ProcessToken::SubShell(
//                                    Shell(vec![
//                                        ShellToken::Process(create_pipe("a")),
//                                        ShellToken::And,
//                                        ShellToken::Process(create_pipe("b")),
//                                        ShellToken::Or,
//                                        ShellToken::Process(create_pipe("c")),
//                                        ShellToken::Or,
//                                        ShellToken::Process(create_pipe("d")),
//                                        ShellToken::And,
//                                        ShellToken::Process(create_pipe("e"))
//                                    ])
//                                )
//                            ])
//                        ])
//                    )
//                ])
//            ))
//        );
    }

    #[test]
    fn test_take_until_and_consume() {
        let r = take_until_and_consume::<_, _, ()>("foo")(&b"abcd foo efgh"[..]);
        assert_eq!(r, Ok((&b" efgh"[..], &b"abcd "[..])));
    }
}