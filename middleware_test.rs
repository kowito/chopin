struct Context<'a> {
    req: &'a str,
}

type Handler = fn(Context) -> String;

#[derive(Clone)]
struct Next<'a, 'b> {
    handler: Handler,
    middlewares: &'a [Middleware],
}

impl<'a, 'b> Next<'a, 'b> {
    fn run(self, ctx: Context<'b>) -> String {
        if let Some((first, rest)) = self.middlewares.split_first() {
            first(ctx, Next { handler: self.handler, middlewares: rest })
        } else {
            (self.handler)(ctx)
        }
    }
}

type Middleware = for<'a, 'b> fn(Context<'b>, Next<'a, 'b>) -> String;

fn my_handler(ctx: Context) -> String {
    format!("handled: {}", ctx.req)
}

fn logger_mw(ctx: Context, next: Next) -> String {
    println!("Before: {}", ctx.req);
    let res = next.run(ctx);
    println!("After: {}", res);
    res
}

fn auth_mw(ctx: Context, next: Next) -> String {
    if ctx.req == "bad" {
        return "401".to_string();
    }
    next.run(ctx)
}

fn main() {
    let mws: Vec<Middleware> = vec![logger_mw, auth_mw];
    
    let ctx1 = Context { req: "good" };
    let next1 = Next { handler: my_handler, middlewares: &mws };
    println!("Result: {}", next1.run(ctx1));
    
    let ctx2 = Context { req: "bad" };
    let next2 = Next { handler: my_handler, middlewares: &mws };
    println!("Result: {}", next2.run(ctx2));
}
