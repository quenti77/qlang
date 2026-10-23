-- Providers: a function that returns a `T` provides a `T`; its parameters are what it needs.

trait Logger
  fun log(self, line: string)
end

struct ConsoleLogger
  public prefix: string = "[app]"
end

impl Logger for ConsoleLogger
  public fun log(self, line: string)
    print("{self.prefix} {line}")
  end
end

struct Config
  public greeting: string
end

struct Greeter
  public config: Config
  public logger: Logger
end

fun make_config() -> Config
  Config { greeting: "Bonjour" }
end

fun make_logger() -> Logger
  ConsoleLogger {}
end

fun make_greeter(config: Config, logger: Logger) -> Greeter
  logger.log("building a greeter")
  Greeter { config, logger }
end

export Logger, ConsoleLogger, Config, Greeter
