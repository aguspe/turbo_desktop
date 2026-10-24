# frozen_string_literal: true

require "test_helper"
require "socket"
require "json"

# Exercises the control channel from the Ruby side against a stub shell, so the
# protocol is tested without building the Tauri app.
class NativeTest < Minitest::Test
  TOKEN = "f" * 64

  def setup
    @received = Queue.new
    @server = TCPServer.new("127.0.0.1", 0)
    @port = @server.addr[1]
    @thread = Thread.new { serve_loop }
    TurboDesktop::Native.channel = nil
  end

  def teardown
    @thread&.kill
    @server&.close
    TurboDesktop::Native.channel = nil
  end

  # ─── outside the shell ──────────────────────────────────────────────────

  def test_is_unavailable_without_a_handshake
    refute_predicate TurboDesktop::Native, :available?
  end

  def test_calls_are_a_no_op_on_the_web_rather_than_an_exception
    # The same Rails app serves the web, where there is no shell. Raising would
    # force a guard at every call site.
    assert_nil TurboDesktop::Native.notify(title: "ignored")
    assert_nil TurboDesktop::Native.call("window", "resize", width: 1)
  end

  # ─── the handshake ──────────────────────────────────────────────────────

  def test_reads_the_handshake_and_becomes_available
    with_handshake
    assert_predicate TurboDesktop::Native, :available?
    assert_equal "http://127.0.0.1:#{@port}", TurboDesktop::Native.channel.url
  end

  def test_consumes_exactly_one_line_of_stdin
    # The shell keeps the pipe open after the handshake so that closing it is
    # the signal to exit. Reading past our line would break that.
    reader, writer = IO.pipe
    writer.puts handshake_line
    writer.puts "reserved for the exit watchdog"
    writer.close

    TurboDesktop::Native.read_handshake!(reader)

    assert_equal "reserved for the exit watchdog", reader.gets.strip
  end

  def test_ignores_a_line_that_is_not_a_handshake
    reader, writer = IO.pipe
    writer.puts "Puma starting in single mode..."
    writer.close

    assert_nil TurboDesktop::Native.read_handshake!(reader)
    refute_predicate TurboDesktop::Native, :available?
  end

  # ─── calls ──────────────────────────────────────────────────────────────

  def test_notify_sends_the_notification_component
    with_handshake
    TurboDesktop::Native.notify(title: "Export finished", body: "invoice.pdf")

    message = @received.pop
    assert_equal "notification", message["component"]
    assert_equal "show", message["event"]
    assert_equal "Export finished", message["data"]["title"]
    assert_equal "invoice.pdf", message["data"]["body"]
  end

  def test_call_reaches_any_component_with_its_payload
    with_handshake
    TurboDesktop::Native.call("window", "resize", width: 1200, height: 900)

    message = @received.pop
    assert_equal "window", message["component"]
    assert_equal({ "width" => 1200, "height" => 900 }, message["data"])
  end

  def test_a_window_resize_reports_the_size_the_shell_actually_applied
    # The shell clamps a resize to the minimums the app configured, so what came
    # back is the only way Ruby knows what it got. (The clamping itself is the
    # shell's, and is tested there; this is the half that has to survive the
    # trip home.)
    with_handshake
    reply = TurboDesktop::Native.call("window", "resize", width: 100, height: 100)

    assert_equal "ok", reply["status"]
    assert_equal 800, reply["width"]
    assert_equal 600, reply["height"]
  end

  def test_a_capability_the_shell_refuses_is_raised_rather_than_returned
    # The shell answers a refused resize with 500 and a reason, and a job that
    # asked for it should hear about that rather than carry on as if it worked.
    with_handshake
    error = assert_raises(TurboDesktop::Native::CallFailed) do
      TurboDesktop::Native.call("window", "resize", width: 0, height: 0)
    end

    assert_match(%r{window/resize failed}, error.message)
    assert_match(/not a usable size/, error.message)
  end

  def test_a_reply_value_comes_back_to_ruby
    with_handshake
    assert_equal "from the clipboard", TurboDesktop::Native.clipboard_read
  end

  # ─── failure ────────────────────────────────────────────────────────────

  def test_a_wrong_token_is_refused_by_the_shell
    with_handshake(token: "0" * 64)
    error = assert_raises(TurboDesktop::Native::CallFailed) do
      TurboDesktop::Native.notify(title: "should not arrive")
    end
    assert_match(/bad token/, error.message)
  end

  def test_a_shell_that_is_not_answering_gives_a_clear_error
    TurboDesktop::Native.channel = TurboDesktop::Native::Channel.new(
      url: "http://127.0.0.1:1", token: TOKEN, header: "x-desktop-token"
    )
    error = assert_raises(TurboDesktop::Native::Error) do
      TurboDesktop::Native.notify(title: "x")
    end
    assert_match(/not answering/, error.message)
  end

  private

  def handshake_line(token: TOKEN)
    JSON.generate(
      protocol: "1.0",
      control: "http://127.0.0.1:#{@port}",
      token: token,
      header: "x-desktop-token"
    )
  end

  def with_handshake(token: TOKEN)
    reader, writer = IO.pipe
    writer.puts handshake_line(token: token)
    TurboDesktop::Native.read_handshake!(reader)
  end

  # A stand-in for the shell's control listener, matching control.rs.
  def serve_loop
    loop do
      conn = @server.accept
      head = +""
      head << conn.readpartial(1) until head.end_with?("\r\n\r\n")
      length = head[/content-length:\s*(\d+)/i, 1].to_i
      body = length.positive? ? conn.read(length) : ""

      if head[/x-desktop-token:\s*(\S+)/i, 1] != TOKEN
        write(conn, "401 Unauthorized", { error: "bad token" })
      elsif head[%r{^POST (\S+)}, 1] != "/invoke"
        write(conn, "404 Not Found", { error: "POST /invoke only" })
      else
        message = JSON.parse(body)
        @received << message
        status, payload = reply_to(message)
        write(conn, status, payload)
      end
      conn.close
    end
  rescue IOError, Errno::EBADF, Errno::ECONNRESET
    nil
  end

  # What the shell answers. Only the window component needs more than "ok":
  # resize reports the size it applied after the app's minimums, and refuses a
  # size nobody could use — both of which Ruby has to carry back to its caller.
  def reply_to(message)
    return [ "200 OK", { status: "ok", text: "from the clipboard" } ] unless
      message["component"] == "window" && message["event"] == "resize"

    width = message["data"]["width"].to_i
    height = message["data"]["height"].to_i
    if width <= 0 || height <= 0
      [ "500 Internal Server Error", { error: "Refused: #{width}x#{height} is not a usable size" } ]
    else
      [ "200 OK", { status: "ok", width: [ width, 800 ].max, height: [ height, 600 ].max } ]
    end
  end

  def write(conn, status, payload)
    json = JSON.generate(payload)
    conn.print "HTTP/1.1 #{status}\r\ncontent-type: application/json\r\n" \
               "content-length: #{json.bytesize}\r\nconnection: close\r\n\r\n#{json}"
  end
end
