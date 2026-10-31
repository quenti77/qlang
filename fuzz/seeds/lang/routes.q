-- Routes déclarées par des annotations : `handle` les trouve par réflexion.
--   qlang serve examples/routes.q --watch

#[annotation]
struct Route
  path: string
  method: string = "GET"
end

#[Route("/")]
fun home(req: Request) -> Response
  Response.text("Accueil")
end

#[Route("/hello")]
fun hello(req: Request) -> Response
  let name = req.query.get("name", "toi")
  Response.text("Bonjour {name} !")
end

#[Route("/echo", method: "POST")]
fun echo(req: Request) -> Response
  let body = req.body
  if body == none then
    return Response.text("")
  end
  Response.text(body)
end

fun handle(req: Request) -> Response
  for info in Reflect.functions() do
    let route = info.annotation("Route")
    if route == none then
      continue
    end
    let path = route.args["path"]
    let method = route.args["method"]
    if path == none or method == none then
      continue
    end
    if path.as_string() == req.path and method.as_string() == req.method then
      let found = Reflect.find<fun(Request) -> Response>(info.name)
      if found != none then
        return found(req)
      end
    end
  end
  Response.not_found()
end
