// Copyright 2018-2022 the Deno authors. All rights reserved. MIT license.

/// An error that occurred while parsing.
#[derive(Debug)]
pub enum ParseError<'a> {
  Backtrace,
  /// Parsing should completely fail.
  Failure(ParseErrorFailure<'a>),
}

/// A complete parsing failure along with the location
/// the error occurred and the error message.
#[derive(Debug)]
pub struct ParseErrorFailure<'a> {
  pub input: &'a str,
  pub message: String,
}

impl<'a> ParseErrorFailure<'a> {
  pub fn new(input: &'a str, message: impl AsRef<str>) -> Self {
    ParseErrorFailure {
      input,
      message: message.as_ref().to_owned(),
    }
  }

  /// Opinionated helper used to fail for trailing input.
  pub fn new_for_trailing_input(input: &'a str) -> Self {
    ParseErrorFailure::new(input, "Unexpected character.")
  }

  /// Opinionated helper to turn this failure into a result.
  pub fn into_result<T>(&self) -> Result<T, ParseErrorFailureError> {
    Err(self.into_error())
  }

  /// Opinionated helper to turn this failure into a `ParseErrorFailureError`.
  pub fn into_error(&self) -> ParseErrorFailureError {
    ParseErrorFailureError(format!(
      "{}\n  {}\n  ~",
      self.message,
      // truncate the output to prevent wrapping in the console
      self.input.chars().take(60).collect::<String>()
    ))
  }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ParseErrorFailureError(String);

impl ParseErrorFailureError {
  pub fn new(message: String) -> Self {
    ParseErrorFailureError(message)
  }
}

impl std::fmt::Display for ParseErrorFailureError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "{}", self.0)
  }
}

impl std::error::Error for ParseErrorFailureError {}

impl<'a> ParseError<'a> {
  pub fn fail<O>(
    input: &'a str,
    message: impl AsRef<str>,
  ) -> ParseResult<'a, O> {
    Err(ParseError::Failure(ParseErrorFailure::new(input, message)))
  }

  pub fn backtrace<O>() -> ParseResult<'a, O> {
    Err(ParseError::Backtrace)
  }
}

pub type ParseResult<'a, O> = Result<(&'a str, O), ParseError<'a>>;

/// Opinionated helper that converts a combinator into a Result<T, String>
pub fn with_failure_handling<'a, T>(
  combinator: impl Fn(&'a str) -> ParseResult<T>,
) -> impl Fn(&'a str) -> Result<T, ParseErrorFailureError> {
  move |input| match combinator(input) {
    Ok((input, result)) => {
      if !input.is_empty() {
        ParseErrorFailure::new_for_trailing_input(input).into_result()
      } else {
        Ok(result)
      }
    }
    Err(ParseError::Backtrace) => {
      ParseErrorFailure::new_for_trailing_input(input).into_result()
    }
    Err(ParseError::Failure(e)) => e.into_result(),
  }
}

/// Recognizes a character.
pub fn ch<'a>(c: char) -> impl Fn(&'a str) -> ParseResult<'a, char> {
  if_true(next_char, move |found_char| *found_char == c)
}

/// Gets the next character.
#[allow(clippy::needless_lifetimes)]
pub fn next_char<'a>(input: &'a str) -> ParseResult<'a, char> {
  match input.chars().next() {
    Some(next_char) => Ok((&input[next_char.len_utf8()..], next_char)),
    _ => ParseError::backtrace(),
  }
}

/// Recognizes any character in the provided string.
pub fn one_of<'a>(
  value: &'static str,
) -> impl Fn(&'a str) -> ParseResult<'a, char> {
  move |input| {
    let (input, c) = next_char(input)?;
    if value.contains(c) {
      Ok((input, c))
    } else {
      ParseError::backtrace()
    }
  }
}

/// Recognizes a string.
pub fn tag<'a>(
  value: impl AsRef<str>,
) -> impl Fn(&'a str) -> ParseResult<'a, &'a str> {
  let value = value.as_ref().to_string();
  move |input| {
    if input.starts_with(&value) {
      Ok((&input[value.len()..], &input[..value.len()]))
    } else {
      Err(ParseError::Backtrace)
    }
  }
}

/// Gets the substring found for the duration of the combinator.
pub fn substring<'a, O>(
  combinator: impl Fn(&'a str) -> ParseResult<'a, O>,
) -> impl Fn(&'a str) -> ParseResult<'a, &'a str> {
  move |input| {
    let original_input = input;
    let (input, _) = combinator(input)?;
    let length = original_input.len() - input.len();
    Ok((input, &original_input[..length]))
  }
}

/// Skip the input while the condition is true.
pub fn skip_while<'a>(
  cond: impl Fn(char) -> bool,
) -> impl Fn(&'a str) -> ParseResult<'a, ()> {
  move |input| {
    for (pos, c) in input.char_indices() {
      if !cond(c) {
        return Ok((&input[pos..], ()));
      }
    }
    // reached the end
    Ok(("", ()))
  }
}

/// Takes a substring while the condition is true.
pub fn take_while<'a>(
  cond: impl Fn(char) -> bool,
) -> impl Fn(&'a str) -> ParseResult<'a, &'a str> {
  substring(skip_while(cond))
}

/// Maps a success to `Some(T)` and a backtrace to `None`.
pub fn maybe<'a, O>(
  combinator: impl Fn(&'a str) -> ParseResult<'a, O>,
) -> impl Fn(&'a str) -> ParseResult<'a, Option<O>> {
  move |input| match combinator(input) {
    Ok((input, value)) => Ok((input, Some(value))),
    Err(ParseError::Backtrace) => Ok((input, None)),
    Err(err) => Err(err),
  }
}

/// Maps the success of a combinator by a function.
pub fn map<'a, O, R>(
  combinator: impl Fn(&'a str) -> ParseResult<'a, O>,
  func: impl Fn(O) -> R,
) -> impl Fn(&'a str) -> ParseResult<'a, R> {
  move |input| {
    let (input, result) = combinator(input)?;
    Ok((input, func(result)))
  }
}

/// Maps the result of a combinator by a function.
pub fn map_res<'a, O, R>(
  combinator: impl Fn(&'a str) -> ParseResult<'a, O>,
  func: impl Fn(ParseResult<'a, O>) -> R,
) -> impl Fn(&'a str) -> R {
  move |input| func(combinator(input))
}

/// Checks for either to match.
pub fn or<'a, O>(
  a: impl Fn(&'a str) -> ParseResult<'a, O>,
  b: impl Fn(&'a str) -> ParseResult<'a, O>,
) -> impl Fn(&'a str) -> ParseResult<'a, O> {
  move |input| match a(input) {
    Ok(result) => Ok(result),
    Err(ParseError::Backtrace) => b(input),
    Err(err) => Err(err),
  }
}

/// Checks for any to match.
pub fn or3<'a, O>(
  a: impl Fn(&'a str) -> ParseResult<'a, O>,
  b: impl Fn(&'a str) -> ParseResult<'a, O>,
  c: impl Fn(&'a str) -> ParseResult<'a, O>,
) -> impl Fn(&'a str) -> ParseResult<'a, O> {
  or(a, or(b, c))
}

/// Checks for any to match.
pub fn or4<'a, O>(
  a: impl Fn(&'a str) -> ParseResult<'a, O>,
  b: impl Fn(&'a str) -> ParseResult<'a, O>,
  c: impl Fn(&'a str) -> ParseResult<'a, O>,
  d: impl Fn(&'a str) -> ParseResult<'a, O>,
) -> impl Fn(&'a str) -> ParseResult<'a, O> {
  or3(a, b, or(c, d))
}

/// Checks for any to match.
pub fn or5<'a, O>(
  a: impl Fn(&'a str) -> ParseResult<'a, O>,
  b: impl Fn(&'a str) -> ParseResult<'a, O>,
  c: impl Fn(&'a str) -> ParseResult<'a, O>,
  d: impl Fn(&'a str) -> ParseResult<'a, O>,
  e: impl Fn(&'a str) -> ParseResult<'a, O>,
) -> impl Fn(&'a str) -> ParseResult<'a, O> {
  or4(a, b, c, or(d, e))
}

/// Checks for any to match.
pub fn or6<'a, O>(
  a: impl Fn(&'a str) -> ParseResult<'a, O>,
  b: impl Fn(&'a str) -> ParseResult<'a, O>,
  c: impl Fn(&'a str) -> ParseResult<'a, O>,
  d: impl Fn(&'a str) -> ParseResult<'a, O>,
  e: impl Fn(&'a str) -> ParseResult<'a, O>,
  f: impl Fn(&'a str) -> ParseResult<'a, O>,
) -> impl Fn(&'a str) -> ParseResult<'a, O> {
  or5(a, b, c, d, or(e, f))
}

/// Checks for any to match.
pub fn or7<'a, O>(
  a: impl Fn(&'a str) -> ParseResult<'a, O>,
  b: impl Fn(&'a str) -> ParseResult<'a, O>,
  c: impl Fn(&'a str) -> ParseResult<'a, O>,
  d: impl Fn(&'a str) -> ParseResult<'a, O>,
  e: impl Fn(&'a str) -> ParseResult<'a, O>,
  f: impl Fn(&'a str) -> ParseResult<'a, O>,
  g: impl Fn(&'a str) -> ParseResult<'a, O>,
) -> impl Fn(&'a str) -> ParseResult<'a, O> {
  or6(a, b, c, d, e, or(f, g))
}

/// Returns the second value and discards the first.
pub fn preceded<'a, First, Second>(
  first: impl Fn(&'a str) -> ParseResult<'a, First>,
  second: impl Fn(&'a str) -> ParseResult<'a, Second>,
) -> impl Fn(&'a str) -> ParseResult<'a, Second> {
  map(pair(first, second), |(_, second)| second)
}

/// Returns the first value and discards the second.
pub fn terminated<'a, First, Second>(
  first: impl Fn(&'a str) -> ParseResult<'a, First>,
  second: impl Fn(&'a str) -> ParseResult<'a, Second>,
) -> impl Fn(&'a str) -> ParseResult<'a, First> {
  map(pair(first, second), |(first, _)| first)
}

/// Gets a second value that is delimited by a first and third.
pub fn delimited<'a, First, Second, Third>(
  first: impl Fn(&'a str) -> ParseResult<'a, First>,
  second: impl Fn(&'a str) -> ParseResult<'a, Second>,
  third: impl Fn(&'a str) -> ParseResult<'a, Third>,
) -> impl Fn(&'a str) -> ParseResult<'a, Second> {
  move |input| {
    let (input, _) = first(input)?;
    let (input, return_value) = second(input)?;
    let (input, _) = third(input)?;
    Ok((input, return_value))
  }
}

/// Returns both results of the two combinators.
pub fn pair<'a, First, Second>(
  first: impl Fn(&'a str) -> ParseResult<'a, First>,
  second: impl Fn(&'a str) -> ParseResult<'a, Second>,
) -> impl Fn(&'a str) -> ParseResult<'a, (First, Second)> {
  move |input| {
    let (input, first_value) = first(input)?;
    let (input, second_value) = second(input)?;
    Ok((input, (first_value, second_value)))
  }
}

/// Asserts that a combinator resolves. If backtracing occurs, returns a failure.
pub fn assert_exists<'a, O>(
  combinator: impl Fn(&'a str) -> ParseResult<'a, O>,
  message: &'static str,
) -> impl Fn(&'a str) -> ParseResult<'a, O> {
  assert(combinator, |result| result.is_ok(), message)
}

/// Asserts that a given condition is true about the combinator.
/// Otherwise returns an error with the message.
pub fn assert<'a, O>(
  combinator: impl Fn(&'a str) -> ParseResult<'a, O>,
  condition: impl Fn(Result<&(&'a str, O), &ParseError<'a>>) -> bool,
  message: &'static str,
) -> impl Fn(&'a str) -> ParseResult<'a, O> {
  move |input| {
    let result = combinator(input);
    if condition(result.as_ref()) {
      result
    } else {
      match combinator(input) {
        Err(ParseError::Failure(err)) => {
          let mut message = message.to_string();
          message.push_str("\n\n");
          message.push_str(&err.message);
          ParseError::fail(err.input, message)
        }
        _ => ParseError::fail(input, message),
      }
    }
  }
}

/// Changes the input on a failure in order to provide
/// a better error message.
pub fn with_failure_input<'a, O>(
  new_input: &'a str,
  combinator: impl Fn(&'a str) -> ParseResult<'a, O>,
) -> impl Fn(&'a str) -> ParseResult<'a, O> {
  move |input| {
    let result = combinator(input);
    match result {
      Err(ParseError::Failure(mut err)) => {
        err.input = new_input;
        Err(ParseError::Failure(err))
      }
      _ => result,
    }
  }
}

/// Provides some context to a failure.
pub fn with_error_context<'a, O>(
  combinator: impl Fn(&'a str) -> ParseResult<'a, O>,
  message: &'static str,
) -> impl Fn(&'a str) -> ParseResult<'a, O> {
  move |input| match combinator(input) {
    Ok(result) => Ok(result),
    Err(ParseError::Backtrace) => Err(ParseError::Backtrace),
    Err(ParseError::Failure(err)) => {
      let mut message = message.to_string();
      message.push_str("\n\n");
      message.push_str(&err.message);
      ParseError::fail(err.input, message)
    }
  }
}

/// Keeps consuming a combinator into an array until a condition
/// is met or backtracing occurs.
pub fn many_till<'a, O, OCondition>(
  combinator: impl Fn(&'a str) -> ParseResult<'a, O>,
  condition: impl Fn(&'a str) -> ParseResult<'a, OCondition>,
) -> impl Fn(&'a str) -> ParseResult<'a, Vec<O>> {
  move |mut input| {
    let mut results = Vec::new();
    while !input.is_empty() && is_backtrace(condition(input))? {
      match combinator(input) {
        Ok((result_input, value)) => {
          results.push(value);
          input = result_input;
        }
        Err(ParseError::Backtrace) => {
          return Ok((input, results));
        }
        Err(err) => return Err(err),
      }
    }
    Ok((input, results))
  }
}

/// Keeps consuming a combinator into an array until a condition
/// is met or backtracing occurs.
pub fn separated_list<'a, O, OSeparator>(
  combinator: impl Fn(&'a str) -> ParseResult<'a, O>,
  separator: impl Fn(&'a str) -> ParseResult<'a, OSeparator>,
) -> impl Fn(&'a str) -> ParseResult<'a, Vec<O>> {
  move |mut input| {
    let mut results = Vec::new();
    while !input.is_empty() {
      match combinator(input) {
        Ok((result_input, value)) => {
          results.push(value);
          input = result_input;
        }
        Err(ParseError::Backtrace) => {
          return Ok((input, results));
        }
        Err(err) => return Err(err),
      }
      input = match separator(input) {
        Ok((input, _)) => input,
        Err(ParseError::Backtrace) => break,
        Err(err) => return Err(err),
      };
    }
    Ok((input, results))
  }
}

/// Applies the combinator 0 or more times and returns a vector
/// of all the parsed results.
pub fn many0<'a, O>(
  combinator: impl Fn(&'a str) -> ParseResult<'a, O>,
) -> impl Fn(&'a str) -> ParseResult<'a, Vec<O>> {
  many_till(combinator, |_| ParseError::backtrace::<()>())
}

/// Applies the combinator at least 1 time, but maybe more
/// and returns a vector of all the parsed results.
pub fn many1<'a, O>(
  combinator: impl Fn(&'a str) -> ParseResult<'a, O>,
) -> impl Fn(&'a str) -> ParseResult<'a, Vec<O>> {
  if_not_empty(many0(combinator))
}

/// Skips the whitespace.
pub fn skip_whitespace(input: &str) -> ParseResult<()> {
  match whitespace(input) {
    Ok((input, _)) => Ok((input, ())),
    // the next char was not a backtrace... continue.
    Err(ParseError::Backtrace) => Ok((input, ())),
    Err(err) => Err(err),
  }
}

/// Parses and expects whitespace.
pub fn whitespace(input: &str) -> ParseResult<&str> {
  if input.is_empty() {
    return ParseError::backtrace();
  }

  for (pos, c) in input.char_indices() {
    if !c.is_whitespace() {
      if pos == 0 {
        return ParseError::backtrace();
      }
      return Ok((&input[pos..], &input[..pos]));
    }
  }

  Ok(("", input))
}

/// Checks if a condition is true for a combinator.
pub fn if_true<'a, O>(
  combinator: impl Fn(&'a str) -> ParseResult<'a, O>,
  condition: impl Fn(&O) -> bool,
) -> impl Fn(&'a str) -> ParseResult<'a, O> {
  move |input| {
    let (input, value) = combinator(input)?;
    if condition(&value) {
      Ok((input, value))
    } else {
      ParseError::backtrace()
    }
  }
}

pub trait IsEmptyable {
  fn is_empty(&self) -> bool;
}

impl IsEmptyable for String {
  fn is_empty(&self) -> bool {
    self.is_empty()
  }
}

impl<'a> IsEmptyable for &'a str {
  fn is_empty(&self) -> bool {
    (*self).is_empty()
  }
}

impl<T> IsEmptyable for Vec<T> {
  fn is_empty(&self) -> bool {
    self.is_empty()
  }
}

/// Checks if the combinator result is not empty.
pub fn if_not_empty<'a, R: IsEmptyable>(
  combinator: impl Fn(&'a str) -> ParseResult<'a, R>,
) -> impl Fn(&'a str) -> ParseResult<'a, R> {
  if_true(combinator, |items| !items.is_empty())
}

/// Checks if a combinator is false without consuming the input.
pub fn check_not<'a, O>(
  combinator: impl Fn(&'a str) -> ParseResult<'a, O>,
) -> impl Fn(&'a str) -> ParseResult<'a, ()> {
  move |input| match combinator(input) {
    Ok(_) => ParseError::backtrace(),
    Err(_) => Ok((input, ())),
  }
}

/// Logs the result for quick debugging purposes.
#[cfg(debug_assertions)]
#[allow(dead_code)]
pub fn log_result<'a, O: std::fmt::Debug>(
  prefix: &'static str,
  combinator: impl Fn(&'a str) -> ParseResult<'a, O>,
) -> impl Fn(&'a str) -> ParseResult<'a, O> {
  move |input| {
    let result = combinator(input);
    println!("{} (input):  {:?}", prefix, input);
    println!("{} (result): {:#?}", prefix, result);
    result
  }
}

fn is_backtrace<O>(result: ParseResult<O>) -> Result<bool, ParseError> {
  match result {
    Ok(_) => Ok(false),
    Err(ParseError::Backtrace) => Ok(true),
    Err(err) => Err(err),
  }
}
