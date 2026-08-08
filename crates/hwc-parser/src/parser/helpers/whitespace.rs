use crate::lexer::Token;
use crate::parser::{ParseError, Parser};
use compact_str::CompactString;

impl<'ast> Parser<'ast> {
    /// Consume a statement terminator (newline, dedent, or EOF)
    ///
    /// In whitespace-significant languages like Python and Hardware Script,
    /// a statement can end in three ways:
    /// 1. Explicit newline (most common)
    /// 2. Dedentation (end of block)
    /// 3. End of file
    /// 4. Next statement starts (after a block expression like match)
    ///
    /// This method handles all cases gracefully. If a newline is present,
    /// it consumes it to keep the stream clean. Otherwise, it just returns Ok()
    /// and lets the main loop continue parsing.
    pub(crate) fn consume_statement_end(&mut self) -> Result<(), ParseError> {
        // If there is a newline, cleanly consume it
        if self.check(&Token::Newline) {
            self.advance();
            return Ok(());
        }

        // If we are at a dedent or EOF, that's fine too
        // (The dedent will be consumed by the parent block parser)
        if self.check(&Token::Dedent) || self.check(&Token::Eof) {
            return Ok(());
        }

        // If a statement ended with a block (like 'match'), the next token
        // will just be the start of the next statement (e.g., an Identifier).
        // This is valid! Just return Ok() and let the main loop continue.
        // If it's actually garbage (e.g., `a = 1 garbage`), the outer loop
        // will naturally fail with "Unexpected identifier 'garbage'".
        Ok(())
    }

    /// Skip newline tokens only
    ///
    /// NOTE: Comments are now automatically skipped by advance() and current(),
    /// so this method only needs to handle newlines.
    pub(crate) fn skip_whitespace(&mut self) {
        while let Some(current) = self.current() {
            match current.token {
                Token::Newline => {
                    self.advance();
                }
                _ => break,
            }
        }
    }

    /// Collect any doc comments/blocks before the current position
    ///
    /// NOTE: This method needs direct token access to collect comments
    /// before they're skipped by the automatic comment filtering.
    pub(crate) fn collect_doc_comments(&mut self) -> Vec<CompactString> {
        let mut docs = Vec::new();

        // Directly access tokens to collect doc comments
        while self.current < self.tokens.len() {
            match &self.tokens[self.current].token {
                Token::DocComment(content) | Token::DocBlock(content) => {
                    docs.push(content.clone().into());
                    self.current += 1; // Use raw increment to bypass comment skipping
                }
                Token::Newline | Token::BlockComment(_) => {
                    self.current += 1; // Use raw increment
                }
                _ => break,
            }
        }

        docs
    }

    pub(crate) fn skip_until_newline(&mut self) {
        while !self.is_at_end() && !self.check(&Token::Newline) && !self.check(&Token::Dedent) {
            self.advance();
        }
    }
}
