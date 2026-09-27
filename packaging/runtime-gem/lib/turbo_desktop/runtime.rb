# frozen_string_literal: true

module TurboDesktop
  # Where the interpreter a packaged app ships actually lives.
  #
  # This gem carries a prebuilt, relocatable Ruby for one platform. Installing
  # it is how a developer gets a runtime to package without building one, and
  # without a compiler: `bundle add turbo_desktop-runtime` resolves to the
  # platform gem for their machine.
  #
  # It is deliberately not the Ruby the app is *developed* with. That comes from
  # the developer's own version manager. This is the artifact that goes inside
  # the bundle.
  module Runtime
    class Missing < StandardError; end

    class << self
      # The interpreter prefix: the directory containing bin/ruby.
      def path
        @path ||= begin
          candidate = File.expand_path("../../runtime/ruby", __dir__)
          unless File.executable?(File.join(candidate, "bin", exe("ruby")))
            raise Missing, <<~MSG
              No interpreter in this gem at #{candidate}.

              turbo_desktop-runtime ships a prebuilt Ruby per platform. Installing
              the plain "ruby" platform gem, or installing on a platform with no
              published build, leaves it empty. Platforms with a build:
                #{SUPPORTED.join(", ")}
            MSG
          end
          candidate
        end
      end

      def available?
        path
        true
      rescue Missing
        false
      end

      def ruby
        File.join(path, "bin", exe("ruby"))
      end

      def version
        @version ||= `#{ruby.inspect} -e 'print RUBY_VERSION'`.strip
      end

      SUPPORTED = %w[
        arm64-darwin x86_64-darwin x86_64-linux aarch64-linux x64-mingw-ucrt
      ].freeze

      private

      def exe(name)
        Gem.win_platform? ? "#{name}.exe" : name
      end
    end
  end
end
