use crossbeam_channel::{unbounded, Receiver, Sender};
use std::ops::Range;

use crate::highlighter::SyntaxHighlighter;
use crate::language::Language;
use crate::token::HighlightToken;

// Background syntax worker
pub struct SyntaxRequest {
    pub buffer_id: usize,
    pub revision: u64,
    pub text: String,
    pub language: Language,
    pub visible_range: Range<usize>,
}

pub struct SyntaxResponse {
    pub buffer_id: usize,
    pub revision: u64,
    pub tokens: Vec<HighlightToken>,
}

pub struct SyntaxWorker {
    req_tx: Sender<SyntaxRequest>,
    res_rx: Receiver<SyntaxResponse>,
}

impl SyntaxWorker {
    pub fn spawn() -> Self {
        let (req_tx, req_rx) = unbounded::<SyntaxRequest>();
        let (res_tx, res_rx) = unbounded::<SyntaxResponse>();

        std::thread::spawn(move || {
            let mut highlighters: std::collections::HashMap<Language, SyntaxHighlighter> =
                std::collections::HashMap::new();

            while let Ok(mut req) = req_rx.recv() {
                while let Ok(newer) = req_rx.try_recv() {
                    req = newer;
                }

                let highlighter = highlighters
                    .entry(req.language)
                    .or_insert_with(|| SyntaxHighlighter::new(req.language).unwrap());

                let base_offset = req.visible_range.start;
                let expected_chunk_len = req
                    .visible_range
                    .end
                    .saturating_sub(req.visible_range.start);
                let slice = if (base_offset == 0 && req.visible_range.end >= req.text.len())
                    || req.text.len() == expected_chunk_len
                {
                    req.text.as_str()
                } else {
                    let start = req.visible_range.start.min(req.text.len());
                    let end = req.visible_range.end.min(req.text.len());
                    &req.text[start..end]
                };

                highlighter.update(slice);
                let mut tokens = highlighter.highlight_range(0, slice.len(), slice);
                if base_offset > 0 {
                    for token in &mut tokens {
                        token.start_byte += base_offset;
                        token.end_byte += base_offset;
                    }
                }

                let _ = res_tx.send(SyntaxResponse {
                    buffer_id: req.buffer_id,
                    revision: req.revision,
                    tokens,
                });
            }
        });

        Self { req_tx, res_rx }
    }

    pub fn send_request(&self, req: SyntaxRequest) {
        let _ = self.req_tx.send(req);
    }

    pub fn try_recv_response(&self) -> Option<SyntaxResponse> {
        self.res_rx.try_recv().ok()
    }
}
