# frozen_string_literal: true

require "json"
require "net/http"
require "uri"

module TurboDesktop
  # Native capabilities, called from Ruby.
  #
  # The bridge runs page-to-shell, which is the right shape on mobile where
  # Hotwire Native has no server to talk to. A desktop app does: its server runs
  # in the same process tree. Without a channel of its own, a background job
  # cannot raise a notification while no page is open, and every native call has
  # to be bounced off a page that may not exist.
  #
  #   TurboDesktop::Native.notify(title: "Export finished", body: "invoice.pdf")
  #   TurboDesktop::Native.call("window", "resize", width: 1200, height: 900)
  #
  # Outside the desktop shell there is no channel, so `available?` is false and
  # calls return nil rather than raising. The same code then runs unchanged on
  # the web, which is the whole point of sharing a Rails app between the two.
  module Native
    class Error < StandardError; end

    # Raised when the shell answers, but the capability itself failed.
    class CallFailed < Error; end

    Channel = Struct.new(:url, :token, :header, keyword_init: true)

    class << self
      # Read the one line of handshake the shell writes to our stdin.
      #
      # One line, then the pipe stays open: the shell holds it so that closing
      # it is the signal to exit, which is the only thing that survives the
      # shell being force-quit. So this consumes exactly one line and leaves the
      # rest of the stream alone.
      def read_handshake!(io = $stdin)
        return channel if channel
        return nil unless io && !io.tty?

        line = begin
          io.gets
        rescue IOError, Errno::EBADF
          nil
        end
        return nil if line.nil? || line.strip.empty?

        parsed = JSON.parse(line)
        return nil unless parsed["control"] && parsed["token"]

        self.channel = Channel.new(
          url: parsed["control"],
          token: parsed["token"],
          header: parsed["header"] || "x-desktop-token"
        )
      rescue JSON::ParserError
        # Not a handshake. A developer running `rails server` by hand sees this,
        # and the app should still boot.
        nil
      end

      attr_accessor :channel

      def available?
        !channel.nil?
      end

      # Send a bridge message and return the shell's reply.
      #
      # Returns nil when there is no shell, so the same call is a no-op on the
      # web rather than an exception to guard at every call site.
      def call(component, event, **data)
        return nil unless available?

        uri = URI.join(channel.url, "/invoke")
        request = Net::HTTP::Post.new(uri)
        request["content-type"] = "application/json"
        request[channel.header] = channel.token
        request.body = JSON.generate(component: component, event: event, data: data)

        response = Net::HTTP.start(uri.hostname, uri.port, open_timeout: 2, read_timeout: 10) do |http|
          http.request(request)
        end

        body = begin
          JSON.parse(response.body.to_s)
        rescue JSON::ParserError
          {}
        end

        unless response.is_a?(Net::HTTPSuccess)
          raise CallFailed, "#{component}/#{event} failed: #{body["error"] || response.code}"
        end

        body
      rescue Errno::ECONNREFUSED, Net::OpenTimeout => e
        raise Error, "the desktop shell is not answering on #{channel&.url}: #{e.class}"
      end

      # ─── Sugar for the common capabilities ─────────────────────────────────

      def notify(title:, body: nil)
        call("notification", "show", title: title, body: body)
      end

      def clipboard_write(text)
        call("clipboard", "write-text", text: text)
      end

      def clipboard_read
        call("clipboard", "read-text")&.fetch("text", nil)
      end

      def open_external(url)
        call("shell", "open-external", url: url)
      end

      def badge(count)
        call("badge", "set", count: count)
      end
    end
  end
end
