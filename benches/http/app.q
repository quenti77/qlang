-- The program `benches/http_load.py` serves and hits.

struct Item
  public id: int
  public name: string
  public tags: array<string>
end

fun fib(n: int) -> int
  if n < 2 then
    return n
  end
  fib(n - 1) + fib(n - 2)
end

fun handle(req: Request) -> Response
  if req.url == "/hello" then
    return Response { status: 200, body: "hello" }
  end
  if req.url == "/json" then
    let items = [Item { id: 1, name: "one", tags: ["a", "b"] }, Item { id: 2, name: "two", tags: [] }]
    return Response { status: 200, headers: { "Content-Type": "application/json" }, body: Json.of(items).to_text() }
  end
  if req.url == "/compute" then
    return Response { status: 200, body: "{fib(18)}" }
  end
  if req.method == "POST" and req.url == "/echo" then
    let body = req.body
    return Response { status: 200, body: if body == none then "" else body end }
  end
  Response { status: 404, body: "not found" }
end
