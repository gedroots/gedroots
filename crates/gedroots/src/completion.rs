//! Autocomplétion de l'éditeur GEDCOM brut : adapte [`gedcom::complete`] au
//! menu de complétion de l'éditeur de gpui-kit.

use gedcom::complete::{self, Catalog, SuggestionKind};
use gpui_kit::component::input::{CompletionProvider, Rope, RopeExt};
use gpui_kit::{App, Result, Task, Window};
use lsp_types::{
    CompletionContext, CompletionItem, CompletionItemKind, CompletionResponse, CompletionTextEdit, Range, TextEdit,
};

pub struct GedcomCompletion {
    catalog: Catalog,
}

impl GedcomCompletion {
    pub fn new(catalog: Catalog) -> Self {
        Self { catalog }
    }
}

impl CompletionProvider for GedcomCompletion {
    fn completions(
        &self,
        rope: &Rope,
        offset: usize,
        _: CompletionContext,
        _: &mut Window,
        _: &mut App,
    ) -> Task<Result<CompletionResponse>> {
        let text = rope.to_string();
        let Some(c) = complete::complete(&text, offset, &self.catalog) else {
            return Task::ready(Ok(CompletionResponse::Array(vec![])));
        };
        let range =
            Range { start: rope.offset_to_position(c.replace.start), end: rope.offset_to_position(c.replace.end) };
        let items = c
            .items
            .into_iter()
            .map(|s| CompletionItem {
                kind: Some(match s.kind {
                    SuggestionKind::Tag => CompletionItemKind::PROPERTY,
                    SuggestionKind::Pointer => CompletionItemKind::REFERENCE,
                    SuggestionKind::Value => CompletionItemKind::ENUM_MEMBER,
                    SuggestionKind::Keyword => CompletionItemKind::KEYWORD,
                    SuggestionKind::Place => CompletionItemKind::VALUE,
                }),
                // La mise en évidence suit la longueur de `filter_text`.
                filter_text: Some(s.label.get(..s.matched).unwrap_or_default().to_string()),
                detail: (!s.detail.is_empty()).then_some(s.detail),
                text_edit: Some(CompletionTextEdit::Edit(TextEdit { range, new_text: s.insert })),
                label: s.label,
                ..Default::default()
            })
            .collect();
        Task::ready(Ok(CompletionResponse::Array(items)))
    }

    fn is_completion_trigger(&self, _offset: usize, new_text: &str, _: &mut App) -> bool {
        // Toute frappe sur une ligne (pas les retours à la ligne ni les collages longs).
        !new_text.is_empty() && new_text.len() <= 4 && !new_text.contains('\n')
    }
}
