//! Contains the binary logic of Ion.
mod designators;
mod prompt;
mod readln;
mod terminate;

use self::{
    prompt::{prompt, prompt_fn},
    readln::readln,
    terminate::terminate_script_quotes,
};
use super::{flags::UNTERMINATED, status::*, FlowLogic, Shell, ShellHistory};
use crate::{parser::Terminator, types};
use liner::{Buffer, Context};
use std::path::Path;

pub const MAN_ION: &str = "NAME
    Ion - The Ion shell

SYNOPSIS
    ion [options] [args...]

DESCRIPTION
    Ion is a commandline shell created to be a faster and easier to use alternative to the
    currently available shells. It is not POSIX compliant.

OPTIONS:
    -c <command>        evaluates given commands instead of reading from the commandline.

    -n or --no-execute
        do not execute any commands, just do syntax checking.

    -v or --version
        prints the version, platform and revision of ion then exits.

ARGS:
    <args>...    Script arguments (@args). If the -c option is not specified, the first
                 parameter is taken as a filename to execute";

pub trait Binary {
    /// Parses and executes the arguments that were supplied to the shell.
    fn execute_script(&mut self, script: &str);
    /// Creates an interactive session that reads from a prompt provided by
    /// Liner.
    fn execute_interactive(self);
    /// Ensures that read statements from a script are terminated.
    fn terminate_script_quotes<I: Iterator<Item = u8>>(&mut self, lines: I) -> i32;
    /// Ion's interface to Liner's `read_line` method, which handles everything related to
    /// rendering, controlling, and getting input from the prompt.
    fn readln(&mut self) -> Option<String>;
    /// Generates the prompt that will be used by Liner.
    fn prompt(&mut self) -> String;
    // Executes the PROMPT function, if it exists, and returns the output.
    fn prompt_fn(&mut self) -> Option<String>;
    // Handles commands given by the REPL, and saves them to history.
    fn save_command(&mut self, command: &str);
    // Resets the flow control fields to their default values.
    fn reset_flow(&mut self);
}

impl Binary for Shell {
    fn save_command(&mut self, cmd: &str) {
        if !cmd.ends_with('/')
            && self
                .variables
                .tilde_expansion(cmd, &self.directory_stack)
                .map_or(false, |path| Path::new(&path).is_dir())
        {
            self.save_command_in_history(&[cmd, "/"].concat());
        } else {
            self.save_command_in_history(cmd);
        }
    }

    fn reset_flow(&mut self) { self.flow_control.reset(); }

    fn execute_interactive(mut self) {
        self.context = Some({
            let mut context = Context::new();
            context.word_divider_fn = Box::new(word_divide);
            if "1" == self.get_str_or_empty("HISTFILE_ENABLED") {
                let path = self.get::<types::Str>("HISTFILE").expect("shell didn't set HISTFILE");
                if !Path::new(path.as_str()).exists() {
                    eprintln!("ion: creating history file at \"{}\"", path);
                }
                let _ = context.history.set_file_name_and_load_history(path.as_str());
            }
            context
        });

        self.evaluate_init_file();

        loop {
            let mut lines = std::iter::repeat_with(|| self.readln())
                .filter_map(|cmd| cmd)
                .flat_map(|s| s.into_bytes().into_iter().chain(Some(b'\n')));
            match Terminator::new(&mut lines).terminate() {
                Some(Ok(command)) => {
                    self.flags &= !UNTERMINATED;
                    let cmd: &str = &designators::expand_designators(&self, command.trim_end());
                    self.on_command(&cmd);
                    self.save_command(&cmd);
                }
                Some(Err(_)) => self.reset_flow(),
                None => {
                    self.flags &= !UNTERMINATED;
                }
            }
        }
    }

    fn execute_script(&mut self, script: &str) {
        self.on_command(script);

        if self.flow_control.unclosed_block() {
            eprintln!(
                "ion: unexpected end of arguments: expected end block for `{}`",
                self.flow_control.block.last().unwrap().short()
            );
            self.exit(FAILURE);
        }
    }

    fn terminate_script_quotes<I: Iterator<Item = u8>>(&mut self, lines: I) -> i32 {
        terminate_script_quotes(self, lines)
    }

    fn readln(&mut self) -> Option<String> { readln(self) }

    fn prompt_fn(&mut self) -> Option<String> { prompt_fn(self) }

    fn prompt(&mut self) -> String { prompt(self) }
}

#[derive(Debug)]
struct WordDivide<I>
where
    I: Iterator<Item = (usize, char)>,
{
    iter:       I,
    count:      usize,
    word_start: Option<usize>,
}
impl<I> WordDivide<I>
where
    I: Iterator<Item = (usize, char)>,
{
    #[inline]
    fn check_boundary(&mut self, c: char, index: usize, escaped: bool) -> Option<(usize, usize)> {
        if let Some(start) = self.word_start {
            if c == ' ' && !escaped {
                self.word_start = None;
                Some((start, index))
            } else {
                self.next()
            }
        } else {
            if c != ' ' {
                self.word_start = Some(index);
            }
            self.next()
        }
    }
}
impl<I> Iterator for WordDivide<I>
where
    I: Iterator<Item = (usize, char)>,
{
    type Item = (usize, usize);

    fn next(&mut self) -> Option<Self::Item> {
        self.count += 1;
        match self.iter.next() {
            Some((i, '\\')) => {
                if let Some((_, cnext)) = self.iter.next() {
                    self.count += 1;
                    // We use `i` in order to include the backslash as part of the word
                    self.check_boundary(cnext, i, true)
                } else {
                    self.next()
                }
            }
            Some((i, c)) => self.check_boundary(c, i, false),
            None => {
                // When start has been set, that means we have encountered a full word.
                self.word_start.take().map(|start| (start, self.count - 1))
            }
        }
    }
}

fn word_divide(buf: &Buffer) -> Vec<(usize, usize)> {
    // -> impl Iterator<Item = (usize, usize)> + 'a
    WordDivide { iter: buf.chars().cloned().enumerate(), count: 0, word_start: None }.collect() // TODO: return iterator directly :D
}
