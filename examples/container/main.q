-- A dependency container written in qlang, on top of `Reflect.callables` and `Any`:
--   qlang run examples/container/main.q
-- Each provider of `providers.q` is found by the type it returns; its parameters are looked up
-- the same way, and every service is built once, the first time it is asked for.
import "providers.q" as providers

struct Container
  providers: map<string, Callable>
  instances: map<string, Any>
end

impl Container
  public fun new(found: array<Callable>) -> Container
    let table: map<string, Callable> = {}
    for c in found do
      table[c.return_type] = c
    end
    Container { providers: table, instances: {} }
  end

  --" The service called `type_name`, built if it was not yet. --"
  public fun resolve(self, type_name: string, wanted: array<string>) -> Result<Any, Error>
    let known = self.instances[type_name]
    if known != none then
      return Result<Any, Error>.Ok(known)
    end
    let provider = self.providers[type_name]
    if provider == none then
      return Result<Any, Error>.Err(Error { message: "no provider for {type_name}" })
    end
    if wanted.contains(type_name) then
      return Result<Any, Error>.Err(Error { message: "{type_name} depends on itself" })
    end
    let args: array<Any> = []
    for p in provider.params do
      args.push(self.resolve(p.type_name, wanted.concat([type_name]))?)
    end
    let made = provider.call(args)?
    self.instances[type_name] = made
    Result<Any, Error>.Ok(made)
  end

  --" The service of type `T`. --"
  public fun get<T>(self) -> Result<T, Error>
    let any = self.resolve(Reflect.type_name<T>(), [])?
    let value = any.as<T>()
    if value == none then
      return Result<T, Error>.Err(Error { message: "{Reflect.type_name<T>()} was provided as {any.type_name()}" })
    end
    Result<T, Error>.Ok(value)
  end
end

let app = Container.new(Reflect.callables(providers))
let greeter = app.get<providers.Greeter>().unwrap()
greeter.logger.log("{greeter.config.greeting} !")
print(app.get<providers.Config>().unwrap().greeting)
print(app.get<providers.Logger>().is_ok())

-- Mistakes are values: a missing provider is an `Err`, not a crash.
print(app.get<providers.ConsoleLogger>().unwrap_err().message)
