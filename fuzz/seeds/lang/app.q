-- Un petit serveur : qlang serve examples/app.q
-- puis, dans un autre terminal :
--   curl -i http://127.0.0.1:8080/hello?name=Ada
--   curl -X POST -d 'salut' http://127.0.0.1:8080/echo

let visits = 0

--" Appelée pour chaque requête. Le code au-dessus ne s'exécute qu'une fois : `visits` est conservée. --"
fun handle(req: Request) -> Response
  visits += 1
  match req.path
    case "/" then
      Response.text("Accueil : {visits} visite(s)\n")
    case "/hello" then
      let name = req.query.get("name", "toi")
      Response.text("Bonjour {name} !\n")
    case "/echo" then
      let body = req.body
      Response.text(if body == none then "" else body end)
    case "/visits" then
      Response.json("\{\"visits\": {visits}\}")
    else
      Response.not_found()
  end
end
