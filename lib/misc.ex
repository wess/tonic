defmodule System do
  def argv, do: :tonic.system_argv()
  def get_env(varname, default \\ nil)
      when is_binary(varname) and (is_binary(default) or is_nil(default)) do
    case :tonic.system_get_env(varname) do
      nil -> default
      other -> other
    end
  end

  def fetch_env(name) do
    case :tonic.system_get_env(name) do
      nil -> :error
      v -> {:ok, v}
    end
  end

  def fetch_env!(name) do
    case :tonic.system_get_env(name) do
      nil -> raise ArgumentError, "could not fetch environment variable #{inspect(name)} because it is not set"
      v -> v
    end
  end

  def put_env(name, value) when is_binary(name) and is_binary(value), do: :tonic.system_put_env(name, value)

  def put_env(enum) do
    Enum.each(enum, fn {k, v} -> put_env(k, v) end)
    :ok
  end

  def get_env, do: Map.new(:tonic.system_env_all())
  def delete_env(varname) when is_binary(varname), do: :tonic.system_delete_env(varname)

  def find_executable(program) when is_binary(program), do: :tonic.system_find_executable(program)
  def tmp_dir, do: :tonic.system_tmp_dir()

  def tmp_dir! do
    tmp_dir() ||
      raise RuntimeError,
            "could not get a writable temporary directory, please set the TMPDIR environment variable"
  end

  def user_home, do: :tonic.system_user_home()

  def user_home! do
    user_home() || raise RuntimeError, "could not find the user home, please set the HOME environment variable"
  end

  def cmd(command, args, opts \\ []) when is_binary(command) and is_list(args) do
    if not Enum.all?(args, &is_binary/1), do: raise(ArgumentError, "all arguments for System.cmd/3 must be binaries")
    exe = if String.contains?(command, "/"), do: command, else: find_executable(command)

    if exe == nil do
      :erlang.error(:enoent)
    end

    env = for {k, v} <- Keyword.get(opts, :env, []), do: {to_string(k), v && to_string(v)}
    cd = Keyword.get(opts, :cd)
    merge = Keyword.get(opts, :stderr_to_stdout, false)

    case :tonic.system_cmd(exe, args, cd && to_string(cd), env, merge) do
      {:error, reason} ->
        :erlang.error(reason)

      {out, status} ->
        into = Keyword.get(opts, :into, "")
        lines = Keyword.get(opts, :lines)

        collected =
          cond do
            into == "" -> out
            lines -> Enum.into(String.split(out, ~r/(?<=\n)/, trim: true) |> Enum.map(&String.trim_trailing(&1, "\n")), into)
            true -> Enum.into([out], into)
          end

        {collected, status}
    end
  end

  def shell(command, opts \\ []) when is_binary(command) do
    cmd("/bin/sh", ["-c", command], opts)
  end

  def pid, do: :tonic.system_getpid()
  def endianness, do: :little
  def compiled_endianness, do: :little
  def stacktrace, do: []
  def no_halt, do: false
  def no_halt(_), do: :ok
  def build_info, do: %{build: "1.18.3 (compiled with Erlang/OTP 27)", date: "", opt_otp_release: "27", otp_release: "27", revision: "", version: "1.18.3"}
  def at_exit(fun) when is_function(fun, 1) do
    Application.put_env(:elixir, :"$at_exit", [fun | Application.get_env(:elixir, :"$at_exit", [])])
    :ok
  end
  def time_offset, do: 0
  def time_offset(_unit), do: 0
  def trap_signal(_signal, _fun), do: {:ok, make_ref()}
  def trap_signal(_signal, _id, _fun), do: {:ok, make_ref()}
  def untrap_signal(_signal, _id), do: :ok

  def halt(status \\ 0)
  def halt(status) when is_integer(status), do: :tonic.system_halt(status)
  def halt(_), do: :tonic.system_halt(1)

  def stop(status \\ 0), do: halt(status)

  def monotonic_time, do: :erlang.monotonic_time(:native)
  def monotonic_time(unit), do: :erlang.monotonic_time(normalize_unit(unit))
  def system_time, do: :erlang.system_time(:native)
  def system_time(unit), do: :erlang.system_time(normalize_unit(unit))
  def os_time, do: :erlang.system_time(:native)
  def os_time(unit), do: :erlang.system_time(normalize_unit(unit))
  def unique_integer(_opts \\ []), do: :tonic.unique_integer()
  def schedulers_online, do: :tonic.schedulers()
  def schedulers, do: :tonic.schedulers()
  def otp_release, do: "27"
  def version, do: "1.18.3"
  def cwd, do: :tonic.file_cwd()
  def cwd!, do: :tonic.file_cwd()

  def convert_time_unit(time, from, to) do
    f = unit_ns(normalize_unit(from))
    t = unit_ns(normalize_unit(to))
    div(time * f, t)
  end

  defp unit_ns(:second), do: 1_000_000_000
  defp unit_ns(:millisecond), do: 1_000_000
  defp unit_ns(:microsecond), do: 1_000
  defp unit_ns(:nanosecond), do: 1
  defp unit_ns(:native), do: 1

  defp normalize_unit(:seconds), do: :second
  defp normalize_unit(:milliseconds), do: :millisecond
  defp normalize_unit(:microseconds), do: :microsecond
  defp normalize_unit(:nanoseconds), do: :nanosecond
  defp normalize_unit(u), do: u
end

defmodule File.CopyError do
  defexception [:reason, :source, :destination, on: "", action: ""]

  def message(exception) do
    formatted = IO.iodata_to_binary(:file.format_error(exception.reason))

    location =
      case exception.on do
        "" -> ""
        on -> ". #{on}"
      end

    "could not #{exception.action} from #{inspect(exception.source)} to " <>
      "#{inspect(exception.destination)}#{location}: #{formatted}"
  end
end

defmodule File.RenameError do
  defexception [:reason, :source, :destination, on: "", action: ""]

  def message(exception) do
    formatted = IO.iodata_to_binary(:file.format_error(exception.reason))

    location =
      case exception.on do
        "" -> ""
        on -> ". #{on}"
      end

    "could not #{exception.action} from #{inspect(exception.source)} to " <>
      "#{inspect(exception.destination)}#{location}: #{formatted}"
  end
end

defmodule File.LinkError do
  defexception [:reason, :existing, :new, action: ""]

  def message(exception) do
    formatted = IO.iodata_to_binary(:file.format_error(exception.reason))

    "could not #{exception.action} from #{inspect(exception.existing)} to " <>
      "#{inspect(exception.new)}: #{formatted}"
  end
end

defmodule File.Stat do
  defstruct [:size, :type, :access, :atime, :mtime, :ctime, :mode, :links, :major_device,
             :minor_device, :inode, :uid, :gid]

  def to_record(%File.Stat{size: size, type: type, access: access, atime: atime, mtime: mtime,
                           ctime: ctime, mode: mode, links: links, major_device: major_device,
                           minor_device: minor_device, inode: inode, uid: uid, gid: gid}) do
    {:file_info, size, type, access, atime, mtime, ctime, mode, links, major_device, minor_device,
     inode, uid, gid}
  end

  def from_record({:file_info, size, type, access, atime, mtime, ctime, mode, links, major_device,
                   minor_device, inode, uid, gid}) do
    # Referencing the module atom keeps __struct_fields__ alive for inspect.
    _ = File.Stat
    %File.Stat{size: size, type: type, access: access, atime: atime, mtime: mtime, ctime: ctime,
               mode: mode, links: links, major_device: major_device, minor_device: minor_device,
               inode: inode, uid: uid, gid: gid}
  end
end

defmodule File.Stream do
  defstruct path: nil, modes: [], line_or_bytes: :line, raw: true, node: nil

  def __build__(path, line_or_bytes, modes) do
    with {:read_offset, offset} <- :lists.keyfind(:read_offset, 1, modes),
         false <- is_integer(offset) and offset >= 0 do
      raise ArgumentError,
            "expected :read_offset to be a non-negative integer, got: #{inspect(offset)}"
    end

    raw = :lists.keyfind(:encoding, 1, modes) == false

    modes =
      case raw do
        true ->
          case :lists.keyfind(:read_ahead, 1, modes) do
            {:read_ahead, false} -> [:raw | :lists.keydelete(:read_ahead, 1, modes)]
            {:read_ahead, _} -> [:raw | modes]
            false -> [:raw, :read_ahead | modes]
          end

        false ->
          modes
      end

    _ = File.Stream
    %File.Stream{path: path, modes: modes, raw: raw, line_or_bytes: line_or_bytes, node: node()}
  end

  def __open__(%File.Stream{path: path}, modes) do
    :file.open(path, Enum.filter(modes, &(not match?({:read_offset, _}, &1))))
  end

  defimpl Collectable do
    def into(%{modes: modes, raw: raw} = stream) do
      modes = for mode <- modes, mode not in [:read], do: mode

      case File.Stream.__open__(stream, [:write | modes]) do
        {:ok, device} ->
          {:ok, into(device, stream, raw)}

        {:error, reason} ->
          raise File.Error, reason: reason, action: "stream", path: stream.path
      end
    end

    defp into(device, stream, raw) do
      fn
        :ok, {:cont, x} ->
          case raw do
            true -> IO.binwrite(device, x)
            false -> IO.write(device, x)
          end

        :ok, :done ->
          :ok = :file.close(device)
          stream

        :ok, :halt ->
          :ok = :file.close(device)
      end
    end
  end

  defimpl Enumerable do
    def reduce(%{modes: modes, line_or_bytes: line_or_bytes, raw: raw} = stream, acc, fun) do
      start_fun = fn ->
        case File.Stream.__open__(stream, read_modes(modes)) do
          {:ok, device} ->
            skip_bom_and_offset(device, raw, modes)

          {:error, reason} ->
            raise File.Error, reason: reason, action: "stream", path: stream.path
        end
      end

      next_fun =
        case raw do
          true -> &IO.each_binstream(&1, line_or_bytes)
          false -> &IO.each_stream(&1, line_or_bytes)
        end

      Stream.resource(start_fun, next_fun, &:file.close/1).(acc, fun)
    end

    def count(%{modes: modes, line_or_bytes: :line, path: path, raw: raw} = stream) do
      pattern = :binary.compile_pattern("\n")

      counter = fn device ->
        device = skip_bom_and_offset(device, raw, modes)
        count_lines(device, path, pattern, read_function(stream), 0)
      end

      {:ok, open!(stream, modes, counter)}
    end

    def count(%{path: path, line_or_bytes: bytes, raw: true, modes: modes} = stream) do
      case File.stat(path) do
        {:ok, %{size: 0}} ->
          {:error, __MODULE__}

        {:ok, %{size: size}} ->
          bom_offset = count_raw_bom(stream, modes)
          offset = get_read_offset(modes)
          size = max(size - bom_offset - offset, 0)
          remainder = if rem(size, bytes) == 0, do: 0, else: 1
          {:ok, div(size, bytes) + remainder}

        {:error, reason} ->
          raise File.Error, reason: reason, action: "stream", path: path
      end
    end

    def count(_stream), do: {:error, __MODULE__}
    def member?(_stream, _term), do: {:error, __MODULE__}
    def slice(_stream), do: {:error, __MODULE__}

    defp open!(stream, modes, fun) do
      case File.Stream.__open__(stream, read_modes(modes)) do
        {:ok, device} ->
          try do
            fun.(device)
          after
            :file.close(device)
          end

        {:error, reason} ->
          raise File.Error, reason: reason, action: "stream", path: stream.path
      end
    end

    defp count_raw_bom(stream, modes) do
      if :trim_bom in modes do
        open!(stream, read_modes(modes), &(&1 |> trim_bom(true) |> elem(1)))
      else
        0
      end
    end

    defp skip_bom_and_offset(device, raw, modes) do
      device =
        if :trim_bom in modes do
          device |> trim_bom(raw) |> elem(0)
        else
          device
        end

      offset = get_read_offset(modes)

      if offset > 0 do
        {:ok, _} = :file.position(device, {:cur, offset})
      end

      device
    end

    defp trim_bom(device, true) do
      bom_length = device |> IO.binread(4) |> bom_length()
      {:ok, new_pos} = :file.position(device, bom_length)
      {device, new_pos}
    end

    defp trim_bom(device, false) do
      case bom_length(IO.read(device, 1)) do
        0 ->
          {:ok, _} = :file.position(device, 0)
          {device, 0}

        _ ->
          {device, 1}
      end
    end

    defp bom_length(<<239, 187, 191, _rest::binary>>), do: 3
    defp bom_length(<<254, 255, _rest::binary>>), do: 2
    defp bom_length(<<255, 254, _rest::binary>>), do: 2
    defp bom_length(<<0, 0, 254, 255, _rest::binary>>), do: 4
    defp bom_length(<<254, 255, 0, 0, _rest::binary>>), do: 4
    defp bom_length(_binary), do: 0

    def get_read_offset(modes) do
      case :lists.keyfind(:read_offset, 1, modes) do
        {:read_offset, offset} -> offset
        false -> 0
      end
    end

    defp read_modes(modes) do
      for mode <- modes, mode not in [:write, :append, :trim_bom], do: mode
    end

    defp count_lines(device, path, pattern, read, count) do
      case read.(device) do
        data when is_binary(data) ->
          count_lines(device, path, pattern, read, count + count_lines(data, pattern))

        :eof ->
          count

        {:error, reason} ->
          raise File.Error, reason: reason, action: "stream", path: path
      end
    end

    defp count_lines(data, pattern), do: length(:binary.matches(data, pattern))

    defp read_function(%{raw: true}), do: &IO.binread(&1, 65536)
    defp read_function(%{raw: false}), do: &IO.read(&1, 65536)
  end
end

defmodule File do
  def regular?(path, opts \\ []) do
    __read_file_type__(IO.chardata_to_string(path), opts) == {:ok, :regular}
  end
  def dir?(path, opts \\ []) do
    __read_file_type__(IO.chardata_to_string(path), opts) == {:ok, :directory}
  end
  def exists?(path, opts \\ []) do
    opts = [{:time, :posix}] ++ opts
    match?({:ok, _}, :file.read_file_info(IO.chardata_to_string(path), opts))
  end
  def mkdir(path) do
    :file.make_dir(IO.chardata_to_string(path))
  end
  def mkdir!(path) do
    case mkdir(path) do
      :ok ->
        :ok

      {:error, reason} ->
        raise File.Error,
          reason: reason,
          action: "make directory",
          path: IO.chardata_to_string(path)
    end
  end
  def mkdir_p(path) do
    do_mkdir_p(IO.chardata_to_string(path))
  end

  defp do_mkdir_p("/") do
    :ok
  end

  defp do_mkdir_p(path) do
    if dir?(path) do
      :ok
    else
      parent = Path.dirname(path)

      if parent == path do
        # Protect against infinite loop
        {:error, :einval}
      else
        _ = do_mkdir_p(parent)

        case :file.make_dir(path) do
          {:error, :eexist} = error ->
            if dir?(path), do: :ok, else: error

          other ->
            other
        end
      end
    end
  end
  def mkdir_p!(path) do
    case mkdir_p(path) do
      :ok ->
        :ok

      {:error, reason} ->
        raise File.Error,
          reason: reason,
          action: "make directory (with -p)",
          path: IO.chardata_to_string(path)
    end
  end
  def read(path) do
    :file.read_file(IO.chardata_to_string(path))
  end
  def read!(path) do
    case read(path) do
      {:ok, binary} ->
        binary

      {:error, reason} ->
        raise File.Error, reason: reason, action: "read file", path: IO.chardata_to_string(path)
    end
  end
  def stat(path, opts \\ []) do
    opts = Keyword.put_new(opts, :time, :universal)

    case :file.read_file_info(IO.chardata_to_string(path), opts) do
      {:ok, fileinfo} ->
        {:ok, File.Stat.from_record(fileinfo)}

      error ->
        error
    end
  end
  def stat!(path, opts \\ []) do
    case stat(path, opts) do
      {:ok, info} ->
        info

      {:error, reason} ->
        raise File.Error,
          reason: reason,
          action: "read file stats",
          path: IO.chardata_to_string(path)
    end
  end
  def lstat(path, opts \\ []) do
    opts = Keyword.put_new(opts, :time, :universal)

    case :file.read_link_info(IO.chardata_to_string(path), opts) do
      {:ok, fileinfo} ->
        {:ok, File.Stat.from_record(fileinfo)}

      error ->
        error
    end
  end
  def lstat!(path, opts \\ []) do
    case lstat(path, opts) do
      {:ok, info} ->
        info

      {:error, reason} ->
        raise File.Error,
          reason: reason,
          action: "read file stats",
          path: IO.chardata_to_string(path)
    end
  end
  def read_link(path) do
    case path |> IO.chardata_to_string() |> :file.read_link() do
      {:ok, target} -> {:ok, IO.chardata_to_string(target)}
      error -> error
    end
  end
  def read_link!(path) do
    case read_link(path) do
      {:ok, resolved} ->
        resolved

      {:error, reason} ->
        raise File.Error, reason: reason, action: "read link", path: IO.chardata_to_string(path)
    end
  end
  def write_stat(path, stat, opts \\ []) do
    opts = Keyword.put_new(opts, :time, :universal)
    :file.write_file_info(IO.chardata_to_string(path), File.Stat.to_record(stat), opts)
  end
  def write_stat!(path, stat, opts \\ []) do
    case write_stat(path, stat, opts) do
      :ok ->
        :ok

      {:error, reason} ->
        raise File.Error,
          reason: reason,
          action: "write file stats",
          path: IO.chardata_to_string(path)
    end
  end
  def touch(path, time \\ System.os_time(:second))

  def touch(path, time) when is_tuple(time) do
    path = IO.chardata_to_string(path)

    with {:error, :enoent} <- __change_time__(1, path, time),
         :ok <- write(path, "", [:append]),
         do: __change_time__(1, path, time)
  end

  def touch(path, time) when is_integer(time) do
    path = IO.chardata_to_string(path)

    with {:error, :enoent} <- __change_time__(2, path, time),
         :ok <- write(path, "", [:append]),
         do: __change_time__(2, path, time)
  end
  def touch!(path, time \\ System.os_time(:second)) do
    case touch(path, time) do
      :ok ->
        :ok

      {:error, reason} ->
        raise File.Error, reason: reason, action: "touch", path: IO.chardata_to_string(path)
    end
  end
  def ln(existing, new) do
    :file.make_link(IO.chardata_to_string(existing), IO.chardata_to_string(new))
  end
  def ln!(existing, new) do
    case ln(existing, new) do
      :ok ->
        :ok

      {:error, reason} ->
        raise File.LinkError,
          reason: reason,
          action: "create hard link",
          existing: IO.chardata_to_string(existing),
          new: IO.chardata_to_string(new)
    end
  end
  def ln_s(existing, new) do
    :file.make_symlink(IO.chardata_to_string(existing), IO.chardata_to_string(new))
  end
  def ln_s!(existing, new) do
    case ln_s(existing, new) do
      :ok ->
        :ok

      {:error, reason} ->
        raise File.LinkError,
          reason: reason,
          action: "create symlink",
          existing: IO.chardata_to_string(existing),
          new: IO.chardata_to_string(new)
    end
  end
  def copy(source, destination, bytes_count \\ :infinity) do
    source = normalize_path_or_io_device(source)
    destination = normalize_path_or_io_device(destination)

    :file.copy(source, destination, bytes_count)
  end
  def copy!(source, destination, bytes_count \\ :infinity) do
    case copy(source, destination, bytes_count) do
      {:ok, bytes_count} ->
        bytes_count

      {:error, reason} ->
        raise File.CopyError,
          reason: reason,
          action: "copy",
          source: normalize_path_or_io_device(source),
          destination: normalize_path_or_io_device(destination)
    end
  end
  def rename(source, destination) do
    source = IO.chardata_to_string(source)
    destination = IO.chardata_to_string(destination)
    :file.rename(source, destination)
  end
  def rename!(source, destination) do
    case rename(source, destination) do
      :ok ->
        :ok

      {:error, reason} ->
        raise File.RenameError,
          reason: reason,
          action: "rename",
          source: IO.chardata_to_string(source),
          destination: IO.chardata_to_string(destination)
    end
  end
  def cp(source_file, destination_file, options \\ [])

  # TODO: Deprecate me on Elixir v1.19
  def cp(source_file, destination_file, callback) when is_function(callback, 2) do
    cp(source_file, destination_file, on_conflict: callback)
  end

  def cp(source_file, destination_file, options) when is_list(options) do
    on_conflict = Keyword.get(options, :on_conflict, fn _, _ -> true end)
    source_file = IO.chardata_to_string(source_file)
    destination_file = IO.chardata_to_string(destination_file)

    case do_cp_file(source_file, destination_file, on_conflict, []) do
      {:error, reason, _} -> {:error, reason}
      _ -> :ok
    end
  end

  defp path_differs?(path, path), do: false

  defp path_differs?(p1, p2) do
    Path.expand(p1) !== Path.expand(p2)
  end
  def cp!(source_file, destination_file, options \\ []) do
    case cp(source_file, destination_file, options) do
      :ok ->
        :ok

      {:error, reason} ->
        raise File.CopyError,
          reason: reason,
          action: "copy",
          source: IO.chardata_to_string(source_file),
          destination: IO.chardata_to_string(destination_file)
    end
  end

  def cp_r(source, destination, options \\ [])

  # TODO: Deprecate me on Elixir v1.19
  def cp_r(source, destination, callback) when is_function(callback, 2) do
    cp_r(source, destination, on_conflict: callback)
  end

  def cp_r(source, destination, options) when is_list(options) do
    on_conflict = Keyword.get(options, :on_conflict, fn _, _ -> true end)
    dereference? = Keyword.get(options, :dereference_symlinks, false)

    source =
      source
      |> IO.chardata_to_string()
      |> assert_no_null_byte!("File.cp_r/3")

    destination =
      destination
      |> IO.chardata_to_string()
      |> assert_no_null_byte!("File.cp_r/3")

    case do_cp_r(source, destination, on_conflict, dereference?, []) do
      {:error, _, _} = error -> error
      res -> {:ok, res}
    end
  end
  def cp_r!(source, destination, options \\ []) do
    case cp_r(source, destination, options) do
      {:ok, files} ->
        files

      {:error, reason, file} ->
        raise File.CopyError,
          reason: reason,
          action: "copy recursively",
          on: file,
          source: IO.chardata_to_string(source),
          destination: IO.chardata_to_string(destination)
    end
  end

  defp do_cp_r(src, dest, on_conflict, dereference?, acc) when is_list(acc) do
    case __read_link_type__(src) do
      {:ok, :regular} ->
        do_cp_file(src, dest, on_conflict, acc)

      {:ok, :symlink} ->
        case :file.read_link(src) do
          {:ok, link} when dereference? ->
            do_cp_r(Path.expand(link, Path.dirname(src)), dest, on_conflict, dereference?, acc)

          {:ok, link} ->
            do_cp_link(link, src, dest, on_conflict, acc)

          {:error, reason} ->
            {:error, reason, src}
        end

      {:ok, :directory} ->
        case :file.list_dir(src) do
          {:ok, files} ->
            case mkdir(dest) do
              success when success in [:ok, {:error, :eexist}] ->
                Enum.reduce(files, [dest | acc], fn x, acc ->
                  do_cp_r(Path.join(src, x), Path.join(dest, x), on_conflict, dereference?, acc)
                end)

              {:error, reason} ->
                {:error, reason, dest}
            end

          {:error, reason} ->
            {:error, reason, src}
        end

      {:ok, _} ->
        {:error, :eio, src}

      {:error, reason} ->
        {:error, reason, src}
    end
  end

  # If we reach this clause, there was an error while processing a file.
  defp do_cp_r(_, _, _, _, acc) do
    acc
  end

  defp copy_file_mode(src, dest) do
    with {:ok, dest_fileinfo} <- stat(dest),
         {:ok, src_fileinfo} <- stat(src) do
      write_stat(dest, %{dest_fileinfo | mode: src_fileinfo.mode})
    end
  end

  # Both src and dest are files.
  defp do_cp_file(src, dest, on_conflict, acc) do
    case :file.copy(src, {dest, [:exclusive]}) do
      {:ok, _} ->
        case copy_file_mode(src, dest) do
          :ok ->
            [dest | acc]

          {:error, reason} ->
            {:error, reason, src}
        end

      {:error, :eexist} ->
        if path_differs?(src, dest) and on_conflict.(src, dest) do
          case copy(src, dest) do
            {:ok, _} ->
              case copy_file_mode(src, dest) do
                :ok ->
                  [dest | acc]

                {:error, reason} ->
                  {:error, reason, src}
              end

            {:error, reason} ->
              {:error, reason, src}
          end
        else
          acc
        end

      {:error, reason} ->
        {:error, reason, src}
    end
  end

  # Both src and dest are files.
  defp do_cp_link(link, src, dest, on_conflict, acc) do
    case :file.make_symlink(link, dest) do
      :ok ->
        [dest | acc]

      {:error, :eexist} ->
        if path_differs?(src, dest) and on_conflict.(src, dest) do
          # If rm/1 fails, :file.make_symlink/2 will fail
          _ = rm(dest)

          case :file.make_symlink(link, dest) do
            :ok -> [dest | acc]
            {:error, reason} -> {:error, reason, src}
          end
        else
          acc
        end

      {:error, reason} ->
        {:error, reason, src}
    end
  end
  def write(path, content, modes \\ []) do
    modes = normalize_modes(modes, false)
    :file.write_file(IO.chardata_to_string(path), content, modes)
  end
  def write!(path, content, modes \\ []) do
    case write(path, content, modes) do
      :ok ->
        :ok

      {:error, reason} ->
        raise File.Error,
          reason: reason,
          action: "write to file",
          path: IO.chardata_to_string(path)
    end
  end
  def rm(path) do
    path = IO.chardata_to_string(path)

    case :file.delete(path) do
      :ok ->
        :ok

      {:error, :eacces} = e ->
        change_mode_windows(path) || e

      {:error, _} = e ->
        e
    end
  end

  defp change_mode_windows(path) do
    if match?({:win32, _}, :os.type()) do
      case :file.read_file_info(path) do
        {:ok, file_info} when elem(file_info, 3) in [:read, :none] ->
          change_mode_windows(path, file_info)

        _ ->
          nil
      end
    end
  end

  defp change_mode_windows(path, file_info) do
    case chmod(path, elem(file_info, 7) + 0o200) do
      :ok -> :file.delete(path)
      {:error, _reason} = error -> error
    end
  end
  def rm!(path) do
    case rm(path) do
      :ok ->
        :ok

      {:error, reason} ->
        raise File.Error, reason: reason, action: "remove file", path: IO.chardata_to_string(path)
    end
  end
  def rmdir(path) do
    :file.del_dir(IO.chardata_to_string(path))
  end
  def rmdir!(path) do
    case rmdir(path) do
      :ok ->
        :ok

      {:error, reason} ->
        raise File.Error,
          reason: reason,
          action: "remove directory",
          path: IO.chardata_to_string(path)
    end
  end
  def rm_rf(path) do
    {major, _} = :os.type()

    path
    |> IO.chardata_to_string()
    |> assert_no_null_byte!("File.rm_rf/1")
    |> do_rm_rf([], major)
  end

  defp do_rm_rf(path, acc, major) do
    case safe_list_dir(path, major) do
      {:ok, files} when is_list(files) ->
        acc =
          Enum.reduce(files, acc, fn file, acc ->
            # In case we can't delete, continue anyway, we might succeed
            # to delete it on Windows due to how they handle symlinks.
            case do_rm_rf(Path.join(path, file), acc, major) do
              {:ok, acc} -> acc
              {:error, _, _} -> acc
            end
          end)

        case rmdir(path) do
          :ok -> {:ok, [path | acc]}
          {:error, :enoent} -> {:ok, acc}
          {:error, reason} -> {:error, reason, path}
        end

      {:ok, :directory} ->
        do_rm_directory(path, acc)

      {:ok, :regular} ->
        do_rm_regular(path, acc)

      {:error, reason} when reason in [:enoent, :enotdir] ->
        {:ok, acc}

      {:error, reason} ->
        {:error, reason, path}
    end
  end

  defp do_rm_regular(path, acc) do
    case rm(path) do
      :ok -> {:ok, [path | acc]}
      {:error, :enoent} -> {:ok, acc}
      {:error, reason} -> {:error, reason, path}
    end
  end

  # On Windows, symlinks are treated as directory and must be removed
  # with rmdir/1. But on Unix-like systems, we remove them via rm/1.
  # So we first try to remove it as a directory and, if we get :enotdir,
  # we fall back to a file removal.
  defp do_rm_directory(path, acc) do
    case rmdir(path) do
      :ok -> {:ok, [path | acc]}
      {:error, :enotdir} -> do_rm_regular(path, acc)
      {:error, :enoent} -> {:ok, acc}
      {:error, reason} -> {:error, reason, path}
    end
  end

  defp safe_list_dir(path, major) do
    case __read_link_type__(path) do
      {:ok, :directory} ->
        # If we cannot read the files, try to delete it anyway
        case :file.list_dir_all(path) do
          {:ok, files} -> {:ok, files}
          {:error, _} -> {:ok, :directory}
        end

      {:ok, :symlink} when major == :win32 ->
        case __read_file_type__(path) do
          {:ok, :directory} -> {:ok, :directory}
          _ -> {:ok, :regular}
        end

      {:ok, _} ->
        {:ok, :regular}

      {:error, :eio} when major == :win32 ->
        # unix domain socket returns `{:error, :eio}`
        # on other platforms the result is `{:ok, :regular}`
        {:ok, :regular}

      {:error, reason} ->
        {:error, reason}
    end
  end
  def rm_rf!(path) do
    case rm_rf(path) do
      {:ok, files} ->
        files

      {:error, reason, _} ->
        raise File.Error,
          reason: reason,
          path: IO.chardata_to_string(path),
          action: "remove files and directories recursively from"
    end
  end
  def open(path, modes_or_function \\ [])

  def open(path, modes) when is_list(modes) do
    :file.open(IO.chardata_to_string(path), normalize_modes(modes, true))
  end

  def open(path, function) when is_function(function, 1) do
    open(path, [], function)
  end
  def open(path, modes, function) when is_list(modes) and is_function(function, 1) do
    case open(path, modes) do
      {:ok, io_device} ->
        try do
          {:ok, function.(io_device)}
        after
          :ok = close(io_device)
        end

      other ->
        other
    end
  end
  def open!(path, modes_or_function \\ []) do
    case open(path, modes_or_function) do
      {:ok, io_device_or_function_result} ->
        io_device_or_function_result

      {:error, reason} ->
        raise File.Error, reason: reason, action: "open", path: IO.chardata_to_string(path)
    end
  end
  def open!(path, modes, function) do
    case open(path, modes, function) do
      {:ok, function_result} ->
        function_result

      {:error, reason} ->
        raise File.Error, reason: reason, action: "open", path: IO.chardata_to_string(path)
    end
  end
  def cwd() do
    case :file.get_cwd() do
      {:ok, base} -> {:ok, IO.chardata_to_string(fix_drive_letter(base))}
      {:error, _} = error -> error
    end
  end

  defp fix_drive_letter([l, ?:, ?/ | rest] = original) when l in ?A..?Z do
    case :os.type() do
      {:win32, _} -> [l + ?a - ?A, ?:, ?/ | rest]
      _ -> original
    end
  end

  defp fix_drive_letter(original), do: original
  def cwd!() do
    case cwd() do
      {:ok, cwd} ->
        cwd

      {:error, reason} ->
        raise File.Error, reason: reason, action: "get current working directory"
    end
  end
  def cd(path) do
    :file.set_cwd(IO.chardata_to_string(path))
  end
  def cd!(path) do
    case cd(path) do
      :ok ->
        :ok

      {:error, reason} ->
        raise File.Error,
          reason: reason,
          action: "set current working directory to",
          path: IO.chardata_to_string(path)
    end
  end
  def cd!(path, function) do
    old = cwd!()
    cd!(path)

    try do
      function.()
    after
      cd!(old)
    end
  end
  def ls(path \\ ".") do
    case :file.list_dir(IO.chardata_to_string(path)) do
      {:ok, file_list} -> {:ok, Enum.map(file_list, &IO.chardata_to_string/1)}
      {:error, _} = error -> error
    end
  end
  def ls!(path \\ ".") do
    case ls(path) do
      {:ok, value} ->
        value

      {:error, reason} ->
        raise File.Error,
          reason: reason,
          action: "list directory",
          path: IO.chardata_to_string(path)
    end
  end
  def close(io_device) do
    :file.close(io_device)
  end
  def stream!(path, line_or_bytes_modes \\ [])

  def stream!(path, modes) when is_list(modes),
    do: stream!(path, :line, modes)

  def stream!(path, line_or_bytes) when is_integer(line_or_bytes) or line_or_bytes == :line,
    do: stream!(path, line_or_bytes, [])
  def stream!(path, line_or_bytes, modes)

  def stream!(path, modes, line_or_bytes) when is_list(modes) do
    # TODO: Deprecate this on Elixir v1.20
    stream!(path, line_or_bytes, modes)
  end

  def stream!(path, line_or_bytes, modes) do
    modes = normalize_modes(modes, true)
    File.Stream.__build__(IO.chardata_to_string(path), line_or_bytes, modes)
  end
  def chmod(path, mode) do
    :file.change_mode(IO.chardata_to_string(path), mode)
  end
  def chmod!(path, mode) do
    case chmod(path, mode) do
      :ok ->
        :ok

      {:error, reason} ->
        raise File.Error,
          reason: reason,
          action: "change mode for",
          path: IO.chardata_to_string(path)
    end
  end
  def chgrp(path, gid) do
    :file.change_group(IO.chardata_to_string(path), gid)
  end
  def chgrp!(path, gid) do
    case chgrp(path, gid) do
      :ok ->
        :ok

      {:error, reason} ->
        raise File.Error,
          reason: reason,
          action: "change group for",
          path: IO.chardata_to_string(path)
    end
  end
  def chown(path, uid) do
    :file.change_owner(IO.chardata_to_string(path), uid)
  end
  def chown!(path, uid) do
    case chown(path, uid) do
      :ok ->
        :ok

      {:error, reason} ->
        raise File.Error,
          reason: reason,
          action: "change owner for",
          path: IO.chardata_to_string(path)
    end
  end

  ## Helpers

  @read_ahead_size 64 * 1024

  defp assert_no_null_byte!(binary, operation) do
    case :binary.match(binary, "\0") do
      {_, _} ->
        raise ArgumentError,
              "cannot execute #{operation} for path with null byte, got: #{inspect(binary)}"

      :nomatch ->
        binary
    end
  end

  defp normalize_modes([:utf8 | rest], binary?) do
    [encoding: :utf8] ++ normalize_modes(rest, binary?)
  end

  defp normalize_modes([:read_ahead | rest], binary?) do
    [read_ahead: @read_ahead_size] ++ normalize_modes(rest, binary?)
  end

  # TODO: Remove :char_list mode on v2.0
  defp normalize_modes([mode | rest], _binary?) when mode in [:charlist, :char_list] do
    if mode == :char_list do
      IO.warn("the :char_list mode is deprecated, use :charlist")
    end

    normalize_modes(rest, false)
  end

  defp normalize_modes([mode | rest], binary?) do
    [mode | normalize_modes(rest, binary?)]
  end

  defp normalize_modes([], true), do: [:binary]
  defp normalize_modes([], false), do: []

  defp normalize_path_or_io_device(path) when is_list(path), do: IO.chardata_to_string(path)
  defp normalize_path_or_io_device(path) when is_binary(path), do: path
  defp normalize_path_or_io_device(io_device) when is_pid(io_device), do: io_device
  defp normalize_path_or_io_device(io_device = {:file_descriptor, _, _}), do: io_device

  defp __read_file_type__(file, opts \\ []) do
    case :file.read_file_info(file, [{:time, :posix} | opts]) do
      {:ok, info} -> {:ok, elem(info, 2)}
      {:error, _} = error -> error
    end
  end

  defp __read_link_type__(file) do
    case :file.read_link_info(file, [{:time, :posix}]) do
      {:ok, info} -> {:ok, elem(info, 2)}
      {:error, _} = error -> error
    end
  end

  defp __change_time__(mode, path, time), do: :tonic.fio_set_times(path, time, time, mode)
end

defmodule Path do
  def absname(path) do
    absname(path, &File.cwd!/0)
  end
  def absname(path, relative_to) do
    path = IO.chardata_to_string(path)

    case type(path) do
      :relative ->
        relative_to =
          if is_function(relative_to, 0) do
            relative_to.()
          else
            relative_to
          end

        absname_join([relative_to, path])

      :absolute ->
        absname_join([path])

      :volumerelative ->
        relative_to =
          if is_function(relative_to, 0) do
            relative_to.()
          else
            relative_to
          end
          |> IO.chardata_to_string()

        absname_vr(split(path), split(relative_to), relative_to)
    end
  end

  # Absolute path on current drive
  defp absname_vr(["/" | rest], [volume | _], _relative), do: absname_join([volume | rest])

  # Relative to current directory on current drive
  defp absname_vr([<<x, ?:>> | rest], [<<x, _::binary>> | _], relative),
    do: absname(absname_join(rest), relative)

  # Relative to current directory on another drive
  defp absname_vr([<<x, ?:>> | name], _, _relative) do
    cwd =
      case :file.get_cwd([x, ?:]) do
        {:ok, dir} -> IO.chardata_to_string(dir)
        {:error, _} -> <<x, ?:, ?/>>
      end

    absname(absname_join(name), cwd)
  end

  @slash [?/, ?\\]

  defp absname_join([]), do: ""
  defp absname_join(list), do: absname_join(list, major_os_type())

  defp absname_join([name1, name2 | rest], os_type) do
    joined = do_absname_join(IO.chardata_to_string(name1), relative(name2), [], os_type)
    absname_join([joined | rest], os_type)
  end

  defp absname_join([name], os_type) do
    do_absname_join(IO.chardata_to_string(name), <<>>, [], os_type)
  end

  defp do_absname_join(<<uc_letter, ?:, rest::binary>>, relativename, [], :win32)
       when uc_letter in ?A..?Z,
       do: do_absname_join(rest, relativename, [?:, uc_letter + ?a - ?A], :win32)

  defp do_absname_join(<<c1, c2, rest::binary>>, relativename, [], :win32)
       when c1 in @slash and c2 in @slash,
       do: do_absname_join(rest, relativename, ~c"//", :win32)

  defp do_absname_join(<<?\\, rest::binary>>, relativename, result, :win32),
    do: do_absname_join(<<?/, rest::binary>>, relativename, result, :win32)

  defp do_absname_join(<<?/, rest::binary>>, relativename, [?., ?/ | result], os_type),
    do: do_absname_join(rest, relativename, [?/ | result], os_type)

  defp do_absname_join(<<?/, rest::binary>>, relativename, [?/ | result], os_type),
    do: do_absname_join(rest, relativename, [?/ | result], os_type)

  defp do_absname_join(<<>>, <<>>, result, os_type),
    do: IO.iodata_to_binary(reverse_maybe_remove_dir_sep(result, os_type))

  defp do_absname_join(<<>>, relativename, [?: | rest], :win32),
    do: do_absname_join(relativename, <<>>, [?: | rest], :win32)

  defp do_absname_join(<<>>, relativename, [?/ | result], os_type),
    do: do_absname_join(relativename, <<>>, [?/ | result], os_type)

  defp do_absname_join(<<>>, relativename, result, os_type),
    do: do_absname_join(relativename, <<>>, [?/ | result], os_type)

  defp do_absname_join(<<char, rest::binary>>, relativename, result, os_type),
    do: do_absname_join(rest, relativename, [char | result], os_type)

  defp reverse_maybe_remove_dir_sep([?/, ?:, letter], :win32), do: [letter, ?:, ?/]
  defp reverse_maybe_remove_dir_sep([?/], _), do: [?/]
  defp reverse_maybe_remove_dir_sep([?/ | name], _), do: :lists.reverse(name)
  defp reverse_maybe_remove_dir_sep(name, _), do: :lists.reverse(name)
  def expand(path) do
    expand_dot(absname(expand_home(path), &File.cwd!/0))
  end
  def expand(path, relative_to) do
    expand_dot(absname(absname(expand_home(path), expand_home(relative_to)), &File.cwd!/0))
  end
  def type(name)
      when is_list(name)
      when is_binary(name) do
    pathtype(name, major_os_type()) |> elem(0)
  end

  # Note this function does not expand paths because the behavior
  # is ambiguous. If we expand it before converting to relative, then
  # "/usr/../../foo" means "/foo". If we expand it after, it means "../foo".
  # We could expand only relative paths but it is best to say it never
  # expands and then provide a `Path.expand_relative` function (or an
  # option) if desired.
  def relative(name) do
    relative(name, major_os_type())
  end

  defp relative(name, os_type) do
    pathtype(name, os_type)
    |> elem(1)
    |> IO.chardata_to_string()
  end

  defp pathtype(name, os_type) do
    case os_type do
      :win32 -> win32_pathtype(name)
      _ -> unix_pathtype(name)
    end
  end

  defp unix_pathtype(path) when path in ["/", ~c"/"], do: {:absolute, "."}
  defp unix_pathtype(<<?/, relative::binary>>), do: {:absolute, relative}
  defp unix_pathtype([?/ | relative]), do: {:absolute, relative}
  defp unix_pathtype([list | rest]) when is_list(list), do: unix_pathtype(list ++ rest)
  defp unix_pathtype(relative), do: {:relative, relative}

  defp win32_pathtype([list | rest]) when is_list(list), do: win32_pathtype(list ++ rest)

  defp win32_pathtype([char, list | rest]) when is_list(list),
    do: win32_pathtype([char | list ++ rest])

  defp win32_pathtype(<<c1, c2, relative::binary>>) when c1 in @slash and c2 in @slash,
    do: {:absolute, relative}

  defp win32_pathtype(<<char, relative::binary>>) when char in @slash,
    do: {:volumerelative, relative}

  defp win32_pathtype(<<_letter, ?:, char, relative::binary>>) when char in @slash,
    do: {:absolute, relative}

  defp win32_pathtype(<<_letter, ?:, relative::binary>>), do: {:volumerelative, relative}

  defp win32_pathtype([c1, c2 | relative]) when c1 in @slash and c2 in @slash,
    do: {:absolute, relative}

  defp win32_pathtype([char | relative]) when char in @slash, do: {:volumerelative, relative}

  defp win32_pathtype([c1, c2, list | rest]) when is_list(list),
    do: win32_pathtype([c1, c2 | list ++ rest])

  defp win32_pathtype([_letter, ?:, char | relative]) when char in @slash,
    do: {:absolute, relative}

  defp win32_pathtype([_letter, ?: | relative]), do: {:volumerelative, relative}
  defp win32_pathtype(relative), do: {:relative, relative}
  def relative_to(path, cwd, opts \\ []) when is_list(opts) do
    os_type = major_os_type()
    split_path = split(path)
    split_cwd = split(cwd)
    force = Keyword.get(opts, :force, false)

    case {split_absolute?(split_path, os_type), split_absolute?(split_cwd, os_type)} do
      {true, true} ->
        split_path = expand_split(split_path)
        split_cwd = expand_split(split_cwd)

        case force do
          true -> relative_to_forced(split_path, split_cwd, split_path)
          false -> relative_to_unforced(split_path, split_cwd, split_path)
        end

      {false, false} ->
        split_path = expand_relative(split_path, [], [])
        split_cwd = expand_relative(split_cwd, [], [])
        relative_to_forced(split_path, split_cwd, [])

      {_, _} ->
        join(expand_relative(split_path, [], []))
    end
  end

  defp relative_to_unforced(path, path, _original), do: "."

  defp relative_to_unforced([h | t1], [h | t2], original),
    do: relative_to_unforced(t1, t2, original)

  defp relative_to_unforced([_ | _] = l1, [], _original), do: join(l1)
  defp relative_to_unforced(_, _, original), do: join(original)

  defp relative_to_forced(path, path, _original), do: "."
  defp relative_to_forced(["."], _path, _original), do: "."
  defp relative_to_forced(path, ["."], _original), do: join(path)
  defp relative_to_forced([h | t1], [h | t2], original), do: relative_to_forced(t1, t2, original)

  # this should only happen if we have two paths on different drives on windows
  defp relative_to_forced(original, _, original), do: join(original)

  defp relative_to_forced(l1, l2, _original) do
    base = List.duplicate("..", length(l2))
    join(base ++ l1)
  end

  defp expand_relative([".." | t], [_ | acc], up), do: expand_relative(t, acc, up)
  defp expand_relative([".." | t], acc, up), do: expand_relative(t, acc, [".." | up])
  defp expand_relative(["." | t], acc, up), do: expand_relative(t, acc, up)
  defp expand_relative([h | t], acc, up), do: expand_relative(t, [h | acc], up)
  defp expand_relative([], [], []), do: ["."]
  defp expand_relative([], acc, up), do: up ++ :lists.reverse(acc)

  defp expand_split([head | tail]), do: expand_split(tail, [head])
  defp expand_split([".." | t], [_, last | acc]), do: expand_split(t, [last | acc])
  defp expand_split([".." | t], acc), do: expand_split(t, acc)
  defp expand_split(["." | t], acc), do: expand_split(t, acc)
  defp expand_split([h | t], acc), do: expand_split(t, [h | acc])
  defp expand_split([], acc), do: :lists.reverse(acc)

  defp split_absolute?(split, :win32), do: win32_split_absolute?(split)
  defp split_absolute?(split, _), do: match?(["/" | _], split)

  defp win32_split_absolute?(["//" | _]), do: true
  defp win32_split_absolute?([<<_, ":/">> | _]), do: true
  defp win32_split_absolute?(_), do: false
  def relative_to_cwd(path, opts \\ []) when is_list(opts) do
    case :file.get_cwd() do
      {:ok, base} -> relative_to(path, IO.chardata_to_string(base), opts)
      _ -> path
    end
  end
  def basename(path) do
    :filename.basename(IO.chardata_to_string(path))
  end
  def basename(path, extension) do
    :filename.basename(IO.chardata_to_string(path), IO.chardata_to_string(extension))
  end
  def dirname(path) do
    :filename.dirname(IO.chardata_to_string(path))
  end
  def extname(path) do
    :filename.extension(IO.chardata_to_string(path))
  end
  def rootname(path) do
    :filename.rootname(IO.chardata_to_string(path))
  end
  def rootname(path, extension) do
    :filename.rootname(IO.chardata_to_string(path), IO.chardata_to_string(extension))
  end
  def join([name1, name2 | rest]), do: join([join(name1, name2) | rest])
  def join([name]), do: IO.chardata_to_string(name)
  def join(left, right) do
    left = IO.chardata_to_string(left)
    os_type = major_os_type()
    do_join(left, right, os_type) |> remove_dir_sep(os_type)
  end

  defp do_join(left, "/", os_type), do: remove_dir_sep(left, os_type)
  defp do_join("", right, os_type), do: relative(right, os_type)
  defp do_join("/", right, os_type), do: "/" <> relative(right, os_type)

  defp do_join(left, right, os_type),
    do: remove_dir_sep(left, os_type) <> "/" <> relative(right, os_type)

  defp remove_dir_sep("", _os_type), do: ""
  defp remove_dir_sep("/", _os_type), do: "/"

  defp remove_dir_sep(bin, os_type) do
    last = :binary.last(bin)

    if last == ?/ or (last == ?\\ and os_type == :win32) do
      binary_part(bin, 0, byte_size(bin) - 1)
    else
      bin
    end
  end
  def split(path) do
    :filename.split(IO.chardata_to_string(path))
  end

  defmodule Wildcard do

    def read_link_info(file) do
      :file.read_link_info(file)
    end

    def read_file_info(file) do
      :file.read_file_info(file)
    end

    def list_dir(dir) do
      case :file.list_dir(dir) do
        {:ok, files} -> {:ok, for(file <- files, hd(file) != ?., do: file)}
        other -> other
      end
    end
  end
  def wildcard(glob, opts \\ []) when is_list(opts) do
    mod = if Keyword.get(opts, :match_dot), do: :file, else: Path.Wildcard

    glob
    |> chardata_to_list!()
    |> :filelib.wildcard(mod)
    |> Enum.map(&IO.chardata_to_string/1)
  end

  defp chardata_to_list!(chardata) do
    case (try do String.to_charlist(IO.chardata_to_string(chardata)) rescue _ -> {:error, "", chardata} end) do
      result when is_list(result) ->
        if 0 in result do
          raise ArgumentError,
                "cannot execute Path.wildcard/2 for path with null byte, got: #{inspect(chardata)}"
        else
          result
        end

      {:error, encoded, rest} ->
        raise UnicodeConversionError, encoded: encoded, rest: rest, kind: :invalid

      {:incomplete, encoded, rest} ->
        raise UnicodeConversionError, encoded: encoded, rest: rest, kind: :incomplete
    end
  end

  defp expand_home(type) do
    case IO.chardata_to_string(type) do
      "~" <> rest -> resolve_home(rest)
      rest -> rest
    end
  end

  defp resolve_home(""), do: user_home!()

  defp resolve_home(rest) do
    case {rest, major_os_type()} do
      {"\\" <> _, :win32} -> user_home!() <> rest
      {"/" <> _, _} -> user_home!() <> rest
      _ -> "~" <> rest
    end
  end

  # expands dots in an absolute path represented as a string
  defp expand_dot(path) do
    [head | tail] = :binary.split(path, "/", [:global])
    IO.iodata_to_binary(expand_dot(tail, [head <> "/"]))
  end

  defp expand_dot([".." | t], [_, _ | acc]), do: expand_dot(t, acc)
  defp expand_dot([".." | t], acc), do: expand_dot(t, acc)
  defp expand_dot(["." | t], acc), do: expand_dot(t, acc)
  defp expand_dot([h | t], acc), do: expand_dot(t, ["/", h | acc])
  defp expand_dot([], ["/", head | acc]), do: :lists.reverse([head | acc])
  defp expand_dot([], acc), do: :lists.reverse(acc)

  defp major_os_type do
    :os.type() |> elem(0)
  end

  # TODO: Deprecate me on Elixir v1.19
  def safe_relative_to(path, cwd) do
    safe_relative(path, cwd)
  end
  def safe_relative(path, relative_to \\ File.cwd!()) do
    path = IO.chardata_to_string(path)

    case :filelib.safe_relative_path(path, relative_to) do
      :unsafe -> :error
      relative_path -> {:ok, IO.chardata_to_string(relative_path)}
    end
  end

  defp user_home!() do
    case System.get_env("HOME") do
      nil -> raise RuntimeError, "could not find the user home, please set the HOME environment variable"
      home -> home
    end
  end
end


defmodule Module do
  def get_attribute(module, name, default \\ nil), do: Tonic.Eval.Modules.get_attribute(module, name) || default
  def put_attribute(module, name, value), do: Tonic.Eval.Modules.put_attribute(module, name, value)
  def delete_attribute(module, name), do: Tonic.Eval.Modules.delete_attribute(module, name)
  def register_attribute(module, name, opts), do: Tonic.Eval.Modules.register_attribute(module, name, opts)

  def concat(a, b) do
    a = strip(a)
    b = strip(b)
    String.to_atom("Elixir." <> Enum.join(Enum.reject([a, b], &(&1 == "")), "."))
  end

  def concat(list) when is_list(list) do
    list |> Enum.map(&strip/1) |> Enum.reject(&(&1 == "")) |> Enum.join(".") |> then(&String.to_atom("Elixir." <> &1))
  end

  defp strip(nil), do: ""
  defp strip(a) when is_atom(a), do: strip(Atom.to_string(a))
  defp strip("Elixir." <> rest), do: rest
  defp strip("Elixir"), do: ""
  defp strip(s) when is_binary(s), do: s

  def split(module) when is_atom(module), do: split(Atom.to_string(module))
  def split("Elixir." <> name), do: String.split(name, ".")
end

defmodule Code do
  def ensure_loaded?(module), do: :tonic.module_loaded(module)
  def ensure_loaded(module), do: if(:tonic.module_loaded(module), do: {:module, module}, else: {:error, :nofile})
  def ensure_compiled(module), do: ensure_loaded(module)

  def eval_string(string, binding \\ [], opts \\ [])

  def eval_string(string, binding, %Macro.Env{} = env) do
    quoted = string_to_quoted!(string, file: env.file, line: env.line)
    Tonic.Eval.eval_quoted(quoted, binding, env)
  end

  def eval_string(string, binding, opts) when is_list(binding) and is_list(opts) do
    file = Keyword.get(opts, :file, "nofile")
    line = Keyword.get(opts, :line, 1)
    quoted = string_to_quoted!(string, file: file, line: line)
    Tonic.Eval.eval_quoted(quoted, binding, opts)
  end

  def eval_quoted(quoted, binding \\ [], opts \\ []) when is_list(binding) do
    Tonic.Eval.eval_quoted(quoted, binding, opts)
  end

  def eval_quoted_with_env(quoted, binding, %Macro.Env{} = env, _opts \\ []) when is_list(binding) do
    Tonic.Eval.eval_quoted_with_env(quoted, binding, env)
  end

  def eval_file(file, relative_to \\ nil) when is_binary(file) do
    file = Path.expand(file, relative_to || File.cwd!())
    eval_string(File.read!(file), [], file: file, line: 1)
  end

  def string_to_quoted(string, opts \\ []) when is_list(opts) do
    file = Keyword.get(opts, :file, "nofile")
    line = Keyword.get(opts, :line, 1)
    column = Keyword.get(opts, :column, 1)

    case :elixir.string_to_tokens(to_charlist(string), line, column, file, opts) do
      {:ok, tokens} ->
        :elixir.tokens_to_quoted(tokens, file, opts)

      {:error, _error_msg} = error ->
        error
    end
  end

  def string_to_quoted!(string, opts \\ []) when is_list(opts) do
    file = Keyword.get(opts, :file, "nofile")
    line = Keyword.get(opts, :line, 1)
    column = Keyword.get(opts, :column, 1)
    :elixir.string_to_quoted!(to_charlist(string), line, column, file, opts)
  end

  def string_to_quoted_with_comments(string, opts \\ []) when is_list(opts) do
    charlist = to_charlist(string)
    file = Keyword.get(opts, :file, "nofile")
    line = Keyword.get(opts, :line, 1)
    column = Keyword.get(opts, :column, 1)

    Process.put(:code_formatter_comments, [])
    opts = [preserve_comments: &preserve_comments/5] ++ opts

    with {:ok, tokens} <- :elixir.string_to_tokens(charlist, line, column, file, opts),
         {:ok, forms} <- :elixir.tokens_to_quoted(tokens, file, opts) do
      comments = Enum.reverse(Process.get(:code_formatter_comments))
      {:ok, forms, comments}
    end
  after
    Process.delete(:code_formatter_comments)
  end

  def string_to_quoted_with_comments!(string, opts \\ []) do
    charlist = to_charlist(string)

    case string_to_quoted_with_comments(charlist, opts) do
      {:ok, forms, comments} ->
        {forms, comments}

      {:error, {location, error, token}} ->
        :elixir_errors.parse_error(
          location,
          Keyword.get(opts, :file, "nofile"),
          error,
          token,
          {charlist, Keyword.get(opts, :line, 1), Keyword.get(opts, :column, 1)}
        )
    end
  end

  defp preserve_comments(line, column, tokens, comment, rest) do
    comments = Process.get(:code_formatter_comments)

    comment = %{
      line: line,
      column: column,
      previous_eol_count: previous_eol_count(tokens),
      next_eol_count: next_eol_count(rest, 0),
      text: List.to_string(comment)
    }

    Process.put(:code_formatter_comments, [comment | comments])
  end

  defp next_eol_count([?\s | rest], count), do: next_eol_count(rest, count)
  defp next_eol_count([?\t | rest], count), do: next_eol_count(rest, count)
  defp next_eol_count([?\n | rest], count), do: next_eol_count(rest, count + 1)
  defp next_eol_count([?\r, ?\n | rest], count), do: next_eol_count(rest, count + 1)
  defp next_eol_count(_, count), do: count

  defp previous_eol_count([{token, {_, _, count}} | _])
       when token in [:eol, :",", :";"] and count > 0 do
    count
  end

  defp previous_eol_count([]), do: 1
  defp previous_eol_count(_), do: 0

  def format_string!(string, opts \\ []) when is_binary(string) and is_list(opts) do
    line_length = Keyword.get(opts, :line_length, 98)

    to_quoted_opts =
      [
        unescape: false,
        literal_encoder: &{:ok, {:__block__, &2, [&1]}},
        token_metadata: true,
        emit_warnings: false
      ] ++ opts

    {forms, comments} = string_to_quoted_with_comments!(string, to_quoted_opts)
    to_algebra_opts = [comments: comments] ++ opts
    doc = Code.Formatter.to_algebra(forms, to_algebra_opts)
    Inspect.Algebra.format(doc, line_length)
  end

  def format_file!(file, opts \\ []) when is_binary(file) and is_list(opts) do
    string = File.read!(file)
    formatted = format_string!(string, [file: file, line: 1] ++ opts)
    [formatted, ?\n]
  end

  def quoted_to_algebra(quoted, opts \\ []) do
    quoted
    |> Code.Normalizer.normalize(opts)
    |> Code.Formatter.to_algebra(opts)
  end
end

defmodule Node do
  def self, do: :nonode@nohost
  def alive?, do: false
  def list, do: []
end
