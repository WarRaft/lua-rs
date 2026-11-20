use crate::ast::Chunk;
use crate::lexer::SPEC;
use crate::printer::build_tree;
use parser_rs::HtmlLang;

/// The viewer for Lua: highlighting by Lua's own lexer.
pub static HTML_LANG: HtmlLang = HtmlLang {
    spec: &SPEC,
    embedded: &[],
};

/// Renders a self-contained HTML page that visualizes a parsed chunk's AST
/// next to its highlighted source; see [`parser_rs::html`].
pub fn render_html(chunk: &Chunk, source: &str, title: &str) -> String {
    parser_rs::render_html(&build_tree(chunk), source, title, &HTML_LANG)
}
