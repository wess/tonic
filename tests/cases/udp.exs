defmodule UdpCase do
  @lo {127, 0, 0, 1}

  def open(opts), do: :gen_udp.open(0, [ip: @lo] ++ opts)

  def port(s) do
    {:ok, p} = :inet.port(s)
    p
  end

  def wait_mailbox(n) do
    case Process.info(self(), :message_queue_len) do
      {:message_queue_len, len} when len >= n -> :ok
      _ ->
        Process.sleep(5)
        wait_mailbox(n)
    end
  end

  def wait_closed(s, tries \\ 200) do
    case :inet.sockname(s) do
      {:error, reason} -> reason
      {:ok, _} when tries > 0 ->
        Process.sleep(5)
        wait_closed(s, tries - 1)
      {:ok, _} -> :still_open
    end
  end

  def flush do
    receive do
      m -> [m | flush()]
    after
      50 -> []
    end
  end

  def run do
    {:ok, a} = open([:binary, active: false])
    {:ok, b} = open(active: false)
    pa = port(a)
    pb = port(b)

    {:ok, {ip, sp}} = :inet.sockname(a)
    IO.inspect({ip, sp == pa, is_integer(pa) and pa > 0}, label: "sockname")
    IO.inspect(:inet.peername(a), label: "peername")
    IO.inspect(:inet.getopts(a, [:active, :mode]), label: "opts a")
    IO.inspect(:inet.getopts(b, [:active, :mode]), label: "opts b")

    # passive mode, list and binary delivery
    IO.inspect(:gen_udp.send(a, @lo, pb, "hello"), label: "send/4")
    {:ok, {from, fp, data}} = :gen_udp.recv(b, 0)
    IO.inspect({from, fp == pa, data}, label: "recv list")
    :ok = :gen_udp.send(b, {@lo, pa}, [?w, "or", [?l, "d"]])
    {:ok, {from, fp, data}} = :gen_udp.recv(a, 0, 1000)
    IO.inspect({from, fp == pb, data}, label: "recv binary")
    :ok = :gen_udp.send(b, ~c"localhost", pa, "by name")
    {:ok, {_, _, data}} = :gen_udp.recv(a, 0, 1000)
    IO.inspect(data, label: "by name")
    IO.inspect(:gen_udp.recv(a, 0, 30), label: "timeout")

    big = :binary.copy("0123456789", 800)
    :ok = :gen_udp.send(a, @lo, pb, big)
    {:ok, {_, _, data}} = :gen_udp.recv(b, 0)
    IO.inspect(length(data), label: "big")
    IO.inspect(:gen_udp.send(a, @lo, pb, :binary.copy("x", 70000)), label: "too big")

    # datagrams keep their boundaries and order
    Enum.each(1..3, fn i -> :ok = :gen_udp.send(a, @lo, pb, "p#{i}") end)
    IO.inspect(for(_ <- 1..3, do: elem(elem(:gen_udp.recv(b, 0), 1), 2)), label: "order")

    # active: :once
    :ok = :inet.setopts(b, [:binary, active: :once])
    IO.inspect(:gen_udp.recv(b, 0, 10), label: "recv while active")
    :ok = :gen_udp.send(a, @lo, pb, "one")
    :ok = :gen_udp.send(a, @lo, pb, "two")

    receive do
      {:udp, ^b, ip, ^pa, data} -> IO.inspect({ip, data}, label: "once")
    end

    receive do
      m -> IO.inspect(m, label: "unexpected")
    after
      50 -> IO.puts("once delivered one")
    end

    IO.inspect(:inet.getopts(b, [:active, :mode]), label: "after once")
    {:ok, {_, _, data}} = :gen_udp.recv(b, 0)
    IO.inspect(data, label: "left for recv")

    # active: true
    :ok = :inet.setopts(b, active: true)
    Enum.each(1..3, fn i -> :ok = :gen_udp.send(a, @lo, pb, "t#{i}") end)

    got =
      for _ <- 1..3 do
        receive do
          {:udp, ^b, _, ^pa, data} -> data
        end
      end

    IO.inspect(got, label: "active true")

    # active: N, then {:udp_passive, socket}
    :ok = :inet.setopts(b, active: false)
    :ok = :inet.setopts(b, active: 2)
    Enum.each(1..3, fn i -> :ok = :gen_udp.send(a, @lo, pb, "n#{i}") end)
    wait_mailbox(3)

    msgs =
      Enum.map(flush(), fn
        {:udp, ^b, _, ^pa, data} -> {:udp, data}
        {:udp_passive, ^b} -> :udp_passive
      end)

    IO.inspect(msgs, label: "active n")
    {:ok, {_, _, data}} = :gen_udp.recv(b, 0)
    IO.inspect(data, label: "after n")
    :ok = :inet.setopts(b, active: 3)
    :ok = :inet.setopts(b, active: -3)
    IO.inspect(flush() == [{:udp_passive, b}], label: "count to zero")

    # a blocked recv does not block other processes
    me = self()

    waiter =
      spawn(fn ->
        {:ok, c} = open([:binary, active: false])
        send(me, {:waiting, port(c)})
        send(me, {:waited, :gen_udp.recv(c, 0)})
      end)

    pc = receive do: ({:waiting, p} -> p)
    Process.sleep(20)
    IO.inspect(Process.alive?(waiter), label: "waiter blocked")
    :ok = :gen_udp.send(a, @lo, pc, "wake")

    receive do
      {:waited, {:ok, {_, ^pa, data}}} -> IO.inspect(data, label: "waiter")
    end

    # controlling_process moves queued datagrams to the new owner
    :ok = :inet.setopts(b, active: true)
    :ok = :gen_udp.send(a, @lo, pb, "queued")
    wait_mailbox(1)

    owner =
      spawn(fn ->
        receive do
          :go ->
            got =
              for _ <- 1..2 do
                receive do
                  {:udp, s, _, _, data} -> {s == b, data}
                end
              end

            send(me, {:new_owner, got})
            send(me, {:owner_check, :gen_udp.controlling_process(b, me)})
        end
      end)

    IO.inspect(:gen_udp.controlling_process(b, owner), label: "controlling_process")
    IO.inspect(:gen_udp.controlling_process(b, self()), label: "not owner")
    IO.inspect(flush(), label: "old owner mailbox")
    :ok = :gen_udp.send(a, @lo, pb, "fresh")
    send(owner, :go)
    receive do: ({:new_owner, got} -> IO.inspect(got, label: "new owner"))
    receive do: ({:owner_check, r} -> IO.inspect(r, label: "handed back"))
    :ok = :gen_udp.send(a, @lo, pb, "back")
    receive do: ({:udp, ^b, _, _, data} -> IO.inspect(data, label: "back home"))

    # sockets close with their owner
    spawn(fn ->
      {:ok, d} = open([])
      send(me, {:opened, d})
    end)

    d = receive do: ({:opened, d} -> d)
    IO.inspect(wait_closed(d), label: "owner exited")

    # address in use, reuseaddr and close
    IO.inspect(:gen_udp.open(pa, ip: @lo), label: "in use")
    {:ok, e} = :gen_udp.open(0, [:binary, :inet, {:reuseaddr, true}, {:recbuf, 65536}, {:ip, :loopback}])
    IO.inspect(:inet.getopts(e, [:reuseaddr, :mode]), label: "reuseaddr")
    IO.inspect(:gen_udp.close(e), label: "close")
    IO.inspect(:gen_udp.close(e), label: "close again")
    IO.inspect(:gen_udp.recv(e, 0), label: "recv closed")
    IO.inspect(:gen_udp.send(e, @lo, pa, "x"), label: "send closed")
    IO.inspect(:inet.sockname(e), label: "sockname closed")
    IO.inspect(:inet.setopts(e, active: true), label: "setopts closed")

    pending =
      spawn(fn ->
        {:ok, f} = open(active: false)
        send(me, {:pending, f})
        send(me, {:pending_result, :gen_udp.recv(f, 0)})
      end)

    f = receive do: ({:pending, f} -> f)
    Process.sleep(20)
    _ = pending
    :ok = :inet.close(f)
    receive do: ({:pending_result, r} -> IO.inspect(r, label: "close during recv"))

    IO.inspect(catch_exit(fn -> :gen_udp.open(0, [:bogus]) end), label: "bad option")
    IO.inspect(catch_exit(fn -> :gen_udp.open(70000) end), label: "bad port")
    :gen_udp.close(a)
    :gen_udp.close(b)
  end

  defp catch_exit(f) do
    try do
      f.()
    catch
      :exit, reason -> {:exit, reason}
    end
  end
end

UdpCase.run()
