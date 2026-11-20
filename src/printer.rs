use crate::ast::*;
pub use parser_rs::Node;

/// Builds a visualizable tree out of a parsed chunk.
pub fn build_tree(chunk: &Chunk) -> Node {
    Node::with_children("Chunk", stmts(&chunk.body)).tag("root", None)
}

/// Renders the chunk's AST as an indented tree.
pub fn render_tree(chunk: &Chunk) -> String {
    parser_rs::render_tree(&build_tree(chunk))
}

/// Renders the chunk's AST as a Graphviz DOT graph.
pub fn render_dot(chunk: &Chunk) -> String {
    parser_rs::render_dot(&build_tree(chunk))
}

fn line_span(line: usize) -> Option<Span> {
    Some(Span::whole_line(line))
}

fn stmts(body: &[Stmt]) -> Vec<Node> {
    body.iter().map(stmt_node).collect()
}

fn container(label: &str, line: usize, children: Vec<Node>) -> Node {
    Node::with_children(label, children).tag("container", line_span(line))
}

fn exprs(values: &[Expr]) -> Vec<Node> {
    values.iter().map(expr_node).collect()
}

fn names(names: &[Name]) -> String {
    names.iter().map(|n| n.name.as_str()).collect::<Vec<_>>().join(", ")
}

fn func_label(func: &Function) -> String {
    let mut params: Vec<&str> = func.params.iter().map(|n| n.name.as_str()).collect();
    if func.vararg {
        params.push("...");
    }
    format!("({})", params.join(", "))
}

fn stmt_node(s: &Stmt) -> Node {
    let line = s.line();
    let (label, kind, children) = match s {
        Stmt::Local { names: ns, values, .. } => {
            let list = ns
                .iter()
                .map(|n| match &n.attrib {
                    Some(a) => format!("{} <{a}>", n.name.name),
                    None => n.name.name.clone(),
                })
                .collect::<Vec<_>>()
                .join(", ");
            (format!("Local {list}"), "stmt", exprs(values))
        }
        Stmt::Assign { targets, values, .. } => (
            "Assign".to_string(),
            "stmt",
            vec![
                container("Targets", line, exprs(targets)),
                container("Values", line, exprs(values)),
            ],
        ),
        Stmt::Call { call, .. } => ("CallStat".to_string(), "stmt", vec![expr_node(call)]),
        Stmt::Do { body, .. } => ("Do".to_string(), "stmt", stmts(body)),
        Stmt::While { cond, body, .. } => (
            "While".to_string(),
            "stmt",
            vec![
                container("Condition", line, vec![expr_node(cond)]),
                container("Body", line, stmts(body)),
            ],
        ),
        Stmt::Repeat { body, cond, .. } => (
            "Repeat".to_string(),
            "stmt",
            vec![
                container("Body", line, stmts(body)),
                container("Until", line, vec![expr_node(cond)]),
            ],
        ),
        Stmt::If {
            branches,
            else_body,
            ..
        } => {
            let mut children = Vec::new();
            for (i, (cond, body)) in branches.iter().enumerate() {
                let kw = if i == 0 { "If" } else { "ElseIf" };
                children.push(container(kw, line, vec![expr_node(cond)]));
                children.push(container("Then", line, stmts(body)));
            }
            if let Some(body) = else_body {
                children.push(container("Else", line, stmts(body)));
            }
            ("If".to_string(), "stmt", children)
        }
        Stmt::NumericFor {
            var,
            start,
            stop,
            step,
            body,
            ..
        } => {
            let mut range = vec![expr_node(start), expr_node(stop)];
            range.extend(step.iter().map(expr_node));
            (
                format!("For {} =", var.name),
                "stmt",
                vec![container("Range", line, range), container("Body", line, stmts(body))],
            )
        }
        Stmt::GenericFor {
            names: ns,
            exprs: es,
            body,
            ..
        } => (
            format!("For {} in", names(ns)),
            "stmt",
            vec![container("Iterator", line, exprs(es)), container("Body", line, stmts(body))],
        ),
        Stmt::Function { name, func, .. } => (
            format!("Function {}{}", name.display(), func_label(func)),
            "sig",
            stmts(&func.body),
        ),
        Stmt::LocalFunction { name, func, .. } => (
            format!("LocalFunction {}{}", name.name, func_label(func)),
            "sig",
            stmts(&func.body),
        ),
        Stmt::Return { values, .. } => ("Return".to_string(), "stmt", exprs(values)),
        Stmt::Break { .. } => ("Break".to_string(), "stmt", Vec::new()),
        Stmt::Goto { label, .. } => (format!("Goto {}", label.name), "stmt", Vec::new()),
        Stmt::Label { name, .. } => (format!("Label {}", name.name), "decl", Vec::new()),
    };
    Node::with_children(format!("{label} (line {line})"), children).tag(kind, line_span(line))
}

fn expr_node(e: &Expr) -> Node {
    let span = Some(e.span);
    let (label, kind, children) = match &e.kind {
        ExprKind::Nil => ("Nil".to_string(), "lit", Vec::new()),
        ExprKind::Bool(b) => (format!("Bool {b}"), "lit", Vec::new()),
        ExprKind::Int(v) => (format!("Int {v}"), "lit", Vec::new()),
        ExprKind::Real(v) => (format!("Real {v}"), "lit", Vec::new()),
        ExprKind::Str(s) => (format!("String {s:?}"), "lit", Vec::new()),
        ExprKind::Vararg => ("Vararg".to_string(), "expr", Vec::new()),
        ExprKind::Function(f) => (format!("Function{}", func_label(f)), "sig", stmts(&f.body)),
        ExprKind::Table(fields) => (
            "Table".to_string(),
            "expr",
            fields
                .iter()
                .map(|f| match f {
                    Field::Positional(v) => expr_node(v),
                    Field::Named(n, v) => Node::with_children(format!("{} =", n.name), vec![expr_node(v)])
                        .tag("container", Some(n.span)),
                    Field::Keyed(k, v) => Node::with_children("[key] =", vec![expr_node(k), expr_node(v)])
                        .tag("container", Some(k.span)),
                })
                .collect(),
        ),
        ExprKind::Name(n) => (format!("Name {n}"), "expr", Vec::new()),
        ExprKind::Index(obj, key) => ("Index".to_string(), "expr", vec![expr_node(obj), expr_node(key)]),
        ExprKind::Field(obj, name) => (format!("Field {name}"), "expr", vec![expr_node(obj)]),
        ExprKind::Call(f, args) => match f.path() {
            Some(path) => (format!("Call {path}"), "expr", exprs(args)),
            None => {
                let mut children = vec![expr_node(f)];
                children.extend(exprs(args));
                ("Call".to_string(), "expr", children)
            }
        },
        ExprKind::MethodCall(obj, name, args) => {
            let mut children = vec![expr_node(obj)];
            children.extend(exprs(args));
            (format!("MethodCall :{name}"), "expr", children)
        }
        ExprKind::Paren(inner) => ("Paren".to_string(), "expr", vec![expr_node(inner)]),
        ExprKind::Unary(op, inner) => (format!("Unary {op:?}"), "expr", vec![expr_node(inner)]),
        ExprKind::Binary(l, op, r) => (format!("Binary {op:?}"), "expr", vec![expr_node(l), expr_node(r)]),
    };
    Node::with_children(label, children).tag(kind, span)
}

#[cfg(test)]
#[path = "printer.test.rs"]
mod tests;
