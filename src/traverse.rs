use crate::ast;
use log::debug;
use std::rc::Rc;

fn prepend<T>(v: Vec<T>, s: &[T]) -> Vec<T>
where
    T: Clone,
{
    let mut tmp: Vec<_> = s.to_owned();
    tmp.extend(v);
    tmp
}

pub struct VisitorContext<'a, T> {
    insert_nodes_after: Vec<T>,
    insert_nodes_before: Vec<T>,
    replace_node: Option<T>,
    insert_new_section: Option<ast::Section>,
    pub curr_funcidx: Option<u32>,
    pub node: &'a T,
}
impl<'a, T> VisitorContext<'a, T> {
    pub fn new(node: &'a T) -> Self {
        Self {
            node,
            insert_nodes_after: vec![],
            insert_nodes_before: vec![],
            insert_new_section: None,
            replace_node: None,
            curr_funcidx: None,
        }
    }
}

impl<'a> VisitorContext<'a, ast::Module> {
    pub fn insert_new_section(&mut self, new_node: ast::Section) {
        self.insert_new_section = Some(new_node);
    }
}

impl<'a, T> VisitorContext<'a, Vec<T>> {
    pub fn insert_node_after(&mut self, new_node: T) {
        self.insert_nodes_after.push(vec![new_node]);
    }

    pub fn insert_node_before(&mut self, new_node: T) {
        self.insert_nodes_before.push(vec![new_node]);
    }
}

impl<'a> VisitorContext<'a, ast::Value<ast::Instr>> {
    pub fn insert_node_after(&mut self, new_node: ast::Instr) {
        self.insert_nodes_after.push(ast::Value::new(new_node));
    }

    pub fn insert_node_before(&mut self, new_node: ast::Instr) {
        self.insert_nodes_before.push(ast::Value::new(new_node));
    }

    pub fn replace_node(&mut self, new_node: ast::Instr) {
        self.replace_node = Some(ast::Value::new(new_node));
    }
}

pub trait Visitor {
    fn visit_instr<'a>(&self, _ctx: &'_ mut VisitorContext<'a, ast::Value<ast::Instr>>) {}
    fn visit_func<'a>(&self, _ctx: &'_ mut VisitorContext<'a, ast::Code>) {}
    fn visit_module<'a>(&self, _ctx: &'_ mut VisitorContext<'a, ast::Module>) {}
    fn visit_code_section<'a>(&self, _ctx: &'_ mut VisitorContext<'a, Vec<ast::Code>>) {}
    fn visit_type_section<'a>(&self, _ctx: &'_ mut VisitorContext<'a, Vec<ast::Type>>) {}
    fn visit_import_section<'a>(&self, _ctx: &'_ mut VisitorContext<'a, Vec<ast::Import>>) {}
    fn visit_func_section<'a>(&self, _ctx: &'_ mut VisitorContext<'a, Vec<u32>>) {}
    fn visit_data_section<'a>(&self, _ctx: &'_ mut VisitorContext<'a, Vec<ast::DataSegment>>) {}
}

pub fn traverse(module: &ast::Module, visitor: Rc<dyn Visitor>) {
    {
        let mut ctx = VisitorContext::new(module);
        Rc::clone(&visitor).visit_module(&mut ctx);
        assert!(ctx.insert_nodes_before.is_empty());
        assert!(ctx.insert_nodes_after.is_empty());

        if let Some(new_section) = ctx.insert_new_section {
            debug!("insert new section: {:?}", new_section);
            module.sections.borrow_mut().push(new_section);
        }
    }

    let mut curr_funcidx = 0;

    for section in module.sections.borrow().iter() {
        match section {
            ast::Section::Func((_section_size, funcs)) => {
                let nodes = funcs.borrow().clone();
                let mut ctx = VisitorContext::new(&nodes);
                Rc::clone(&visitor).visit_func_section(&mut ctx);
                assert!(ctx.insert_nodes_before.is_empty());

                let mut new_nodes = ctx.insert_nodes_after;
                new_nodes.reverse();

                for new_node in new_nodes {
                    debug!("inject new func: {:?}", new_node);
                    funcs.borrow_mut().extend_from_slice(&new_node);
                }
            }
            ast::Section::Type((_section_size, types)) => {
                let nodes = types.borrow().clone();
                let mut ctx = VisitorContext::new(&nodes);
                Rc::clone(&visitor).visit_type_section(&mut ctx);
                assert!(ctx.insert_nodes_before.is_empty());

                let mut new_nodes = ctx.insert_nodes_after;
                new_nodes.reverse();

                for new_node in new_nodes {
                    debug!("inject new type: {:?}", new_node);
                    types.borrow_mut().extend_from_slice(&new_node);
                }
            }
            ast::Section::Import((_section_size, content)) => {
                let nodes = content.borrow().clone();
                let mut ctx = VisitorContext::new(&nodes);
                Rc::clone(&visitor).visit_import_section(&mut ctx);
                assert!(ctx.insert_nodes_after.is_empty());

                {
                    let mut new_nodes = ctx.insert_nodes_before;
                    new_nodes.reverse();

                    for new_node in new_nodes {
                        debug!("inject new import: {:?}", new_node);
                        let new = prepend(new_node, &content.borrow().clone());
                        *content.borrow_mut() = new;
                    }
                }

                curr_funcidx += content.borrow().len() as u32;
            }
            ast::Section::Code((_section_size, codes)) => {
                {
                    let nodes = codes.borrow().clone().value;
                    let mut ctx = VisitorContext::new(&nodes);
                    Rc::clone(&visitor).visit_code_section(&mut ctx);
                    assert!(ctx.insert_nodes_before.is_empty());

                    let mut new_nodes = ctx.insert_nodes_after;
                    new_nodes.reverse();

                    for new_node in new_nodes {
                        debug!("inject new code: {:?}", new_node);
                        codes.borrow_mut().value.extend_from_slice(&new_node);
                    }
                }

                for code in codes.borrow().value.iter() {
                    {
                        let mut ctx = VisitorContext::new(code);
                        Rc::clone(&visitor).visit_func(&mut ctx);

                        assert!(ctx.insert_nodes_before.is_empty());
                        assert!(ctx.insert_nodes_after.is_empty());
                    }

                    visit_expr(Rc::clone(&code.body), Rc::clone(&visitor), curr_funcidx);

                    curr_funcidx += 1;
                }
            }
            ast::Section::Data((_section_size, segments)) => {
                let nodes = segments.borrow().clone();
                let mut ctx = VisitorContext::new(&nodes);
                Rc::clone(&visitor).visit_data_section(&mut ctx);
                assert!(ctx.insert_nodes_before.is_empty());

                let mut new_nodes = ctx.insert_nodes_after;
                new_nodes.reverse();

                for new_node in new_nodes {
                    debug!("inject new data: {:?}", new_node);
                    segments.borrow_mut().extend_from_slice(&new_node);
                }
            }
            _ => {}
        }
    }
}

fn visit_expr(
    expr: ast::MutableValue<Vec<ast::Value<ast::Instr>>>,
    visitor: Rc<dyn Visitor>,
    curr_funcidx: u32,
) {
    let expr_copy = expr.borrow().clone();

    for i in 0..expr_copy.value.len() {
        let instr = expr_copy.value[i].clone();
        if let ast::Instr::Block(_, body) = instr.value {
            visit_expr(body, visitor.clone(), curr_funcidx);
        } else if let ast::Instr::If(_, body) = instr.value {
            visit_expr(body, visitor.clone(), curr_funcidx);
        } else if let ast::Instr::Loop(_, body) = instr.value {
            visit_expr(body, visitor.clone(), curr_funcidx);
        } else {
            let mut ctx = VisitorContext::new(&instr);
            ctx.curr_funcidx = Some(curr_funcidx);
            visitor.visit_instr(&mut ctx);

            if let Some(replace_node) = ctx.replace_node {
                debug!("replace instr: {:?}", replace_node);
                expr.borrow_mut().value[i] = replace_node;
            }

            {
                let mut new_nodes = ctx.insert_nodes_after;
                new_nodes.reverse();

                for new_node in new_nodes {
                    debug!("insert instr: {:?}", new_node);
                    expr.borrow_mut().value.insert(i + 1, new_node);
                }
            }

            {
                let mut new_nodes = ctx.insert_nodes_before;
                new_nodes.reverse();

                for new_node in new_nodes {
                    debug!("insert instr: {:?}", new_node);
                    expr.borrow_mut().value.insert(i, new_node);
                }
            }
        }
    }
}
