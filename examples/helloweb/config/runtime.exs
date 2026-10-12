import Config

config :helloweb, Helloweb.Repo, database: System.get_env("DATABASE", "helloweb.db")
config :helloweb, port: String.to_integer(System.get_env("PORT", "4000"))
