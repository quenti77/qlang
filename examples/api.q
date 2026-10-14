-- Une petite API avec un routeur : qlang serve examples/api.q
--   curl http://127.0.0.1:8080/items
--   curl -X POST http://127.0.0.1:8080/items/ordinateur
--   curl http://127.0.0.1:8080/items/ordinateur

let items: array<string> = []
let router = Router.new()

router.get("/items", fun(req: Request) -> Response
  Response.json("[" + items.map(fun(s: string) -> string json_escape(s) end).join(",") + "]")
end)

router.get("/items/:name", fun(req: Request) -> Response
  let name = req.params.get("name", "")
  if items.contains(name) then
    Response.text(name)
  else
    Response.not_found()
  end
end)

router.post("/items/:name", fun(req: Request) -> Response
  items[] = req.params.get("name", "")
  Response.text("ajouté").with_status(201)
end)

fun handle(req: Request) -> Response
  router.handle(req)
end
