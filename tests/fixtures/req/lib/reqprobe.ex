defmodule ReqProbe do
  def main([url]) do
    Logger.configure(level: :warning)
    response = Req.get!(url <> "/json", params: [answer: "42"],
      headers: [{"x-portable", "yes"}], retry: false)
    IO.inspect({response.status, response.body["answer"], response.body["header"]})
    response = Req.post!(url <> "/echo", json: %{portable: true}, retry: false)
    IO.inspect({response.status, response.body["portable"]})
    response = Req.get!(url <> "/redirect", retry: false)
    IO.inspect({response.status, response.body})
    IO.inspect(Req.get!(url <> "/status", retry: false).status)
  end
end
