-- Un petit serveur : qlang serve examples/app.q
-- puis, dans un autre terminal : curl -i http://127.0.0.1:8080/hello

let visits = 0

--" Appelée pour chaque requête. Le code au-dessus ne s'exécute qu'une fois : `visits` est conservée. --"
fun handle(req: Request) -> Response
  visits += 1
  match req.url
    case "/" then
      Response { status: 200, body: "Accueil : {visits} visite(s)\n" }
    case "/hello" then
      Response { status: 200, body: "Bonjour !\n" }
    case "/echo" then
      let body = req.body
      let text = if body == none then "" else body end
      Response { status: 200, headers: { "Content-Type": "text/plain; charset=utf-8" }, body: text }
    else
      Response { status: 404, body: "Introuvable : {req.url}\n" }
  end
end
