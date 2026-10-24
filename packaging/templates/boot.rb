# Boot Puma from config.ru on a port the OS picks, and write one line of
# handshake where the shell can read it. See pack.sh for why `rails server` is
# avoided.
require "json"
require "rack"
require "puma"
require "puma/configuration"
require "puma/launcher"

# Keep a private handle on the real stdout, then point $stdout at stderr.
# Everything Puma prints — the banner especially — goes to stderr, and the
# handshake channel stays clean. Doing it here rather than through Puma's
# log_writer keeps this independent of Puma's API, which has moved around.
handshake = $stdout.dup
handshake.sync = true
$stdout.reopen($stderr)

app, _ = Rack::Builder.parse_file(File.expand_path("config.ru", __dir__))

config = Puma::Configuration.new do |c|
  c.bind "tcp://127.0.0.1:0"     # the OS picks, which removes the pick-a-port race
  c.app app
  c.workers 0                    # one process to supervise, and one to reap
  c.threads 1, 5
  c.log_requests false
end

launcher = Puma::Launcher.new(config)

# Puma 8 renamed on_booted to after_booted, and the bound port comes from
# binder.connected_ports — binder.full_urls does not exist. An exception raised
# in this hook is swallowed by Puma's event loop, so it is reported explicitly
# rather than leaving a server running that never announced itself.
hook = launcher.events.respond_to?(:after_booted) ? :after_booted : :on_booted
launcher.events.public_send(hook) do
  begin
    port = launcher.binder.connected_ports.first
    raise "Puma bound no TCP port" unless port
    handshake.puts JSON.generate(protocol: "1.0", url: "http://127.0.0.1:#{port}", pid: Process.pid)
  rescue => e
    warn "[boot] could not announce the server: #{e.class}: #{e.message}"
    Process.exit!(1)
  end
end

# The rule that prevents an orphaned server: exit when the parent closes stdin.
# It is the only layer that survives the parent being force-quit, since no shell
# code runs then. Skipped when stdin is not a pipe, so the bundle stays runnable
# by hand for debugging.
if $stdin.stat.pipe? || !$stdin.tty?
  Thread.new do
    $stdin.read
    Process.exit!(0)
  end
end

launcher.run
