use crossbeam_channel::{unbounded, Receiver, Sender};
use ropey::Rope;
use std::collections::{BTreeMap, HashMap};
use std::ops::Range;

use crate::highlighter::SyntaxHighlighter;
use crate::language::Language;
use crate::token::HighlightToken;

pub trait IntoRope {
    fn into_rope(self) -> Rope;
}

impl IntoRope for Rope {
    fn into_rope(self) -> Rope {
        self
    }
}

impl IntoRope for String {
    fn into_rope(self) -> Rope {
        Rope::from_str(&self)
    }
}

impl IntoRope for &str {
    fn into_rope(self) -> Rope {
        Rope::from_str(self)
    }
}

// Background syntax worker
pub struct SyntaxRequest<T = Rope> {
    pub buffer_id: usize,
    pub revision: u64,
    pub text: T,
    pub language: Language,
    pub visible_range: Range<usize>,
}

impl<T: IntoRope> SyntaxRequest<T> {
    pub fn into_rope_request(self) -> SyntaxRequest<Rope> {
        SyntaxRequest {
            buffer_id: self.buffer_id,
            revision: self.revision,
            text: self.text.into_rope(),
            language: self.language,
            visible_range: self.visible_range,
        }
    }
}

pub struct SyntaxResponse {
    pub buffer_id: usize,
    pub revision: u64,
    pub tokens: Vec<HighlightToken>,
}

pub enum SyntaxCommand {
    Request(SyntaxRequest<Rope>),
    DropBuffer(usize),
}

pub struct SyntaxWorker {
    cmd_tx: Sender<SyntaxCommand>,
    res_rx: Receiver<SyntaxResponse>,
}

impl SyntaxWorker {
    pub fn spawn() -> Self {
        let (cmd_tx, cmd_rx) = unbounded::<SyntaxCommand>();
        let (res_tx, res_rx) = unbounded::<SyntaxResponse>();

        std::thread::spawn(move || {
            let mut highlighters: HashMap<(usize, Language), SyntaxHighlighter> = HashMap::new();

            while let Ok(first_cmd) = cmd_rx.recv() {
                let mut pending: BTreeMap<usize, SyntaxRequest<Rope>> = BTreeMap::new();

                let process_cmd =
                    |cmd: SyntaxCommand,
                     highlighters: &mut HashMap<(usize, Language), SyntaxHighlighter>,
                     pending: &mut BTreeMap<usize, SyntaxRequest<Rope>>| {
                        match cmd {
                            SyntaxCommand::DropBuffer(buffer_id) => {
                                pending.remove(&buffer_id);
                                highlighters.retain(|&(b_id, _), _| b_id != buffer_id);
                            }
                            SyntaxCommand::Request(req) => {
                                pending.insert(req.buffer_id, req);
                            }
                        }
                    };

                process_cmd(first_cmd, &mut highlighters, &mut pending);
                while let Ok(cmd) = cmd_rx.try_recv() {
                    process_cmd(cmd, &mut highlighters, &mut pending);
                }

                for req in pending.into_values() {
                    let highlighter = highlighters
                        .entry((req.buffer_id, req.language))
                        .or_insert_with(|| {
                            SyntaxHighlighter::new(req.language)
                                .unwrap_or_else(|_| SyntaxHighlighter::plain(req.language))
                        });

                    let text_str = req.text.to_string();
                    let base_offset = req.visible_range.start;
                    let expected_chunk_len = req
                        .visible_range
                        .end
                        .saturating_sub(req.visible_range.start);
                    let (slice, actual_offset) = if (base_offset == 0
                        && req.visible_range.end >= text_str.len())
                        || text_str.len() == expected_chunk_len
                    {
                        (text_str.as_str(), base_offset)
                    } else {
                        let start =
                            crate::lexer::floor_char_boundary(&text_str, req.visible_range.start);
                        let end =
                            crate::lexer::floor_char_boundary(&text_str, req.visible_range.end)
                                .max(start);
                        (&text_str[start..end], start)
                    };

                    highlighter.update(slice);
                    let mut tokens = highlighter.highlight_range(0, slice.len(), slice);
                    if actual_offset > 0 {
                        for token in &mut tokens {
                            token.start_byte += actual_offset;
                            token.end_byte += actual_offset;
                        }
                    }

                    let _ = res_tx.send(SyntaxResponse {
                        buffer_id: req.buffer_id,
                        revision: req.revision,
                        tokens,
                    });
                }
            }
        });

        Self { cmd_tx, res_rx }
    }

    pub fn send_request<T: IntoRope>(&self, req: SyntaxRequest<T>) {
        let _ = self
            .cmd_tx
            .send(SyntaxCommand::Request(req.into_rope_request()));
    }

    pub fn drop_buffer(&self, buffer_id: usize) {
        let _ = self.cmd_tx.send(SyntaxCommand::DropBuffer(buffer_id));
    }

    pub fn try_recv_response(&self) -> Option<SyntaxResponse> {
        self.res_rx.try_recv().ok()
    }
}
