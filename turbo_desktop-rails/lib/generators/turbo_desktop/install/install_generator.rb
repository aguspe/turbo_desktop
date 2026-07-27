require "rails/generators/base"

module TurboDesktop
  module Generators
    class InstallGenerator < Rails::Generators::Base
      source_root File.expand_path("templates", __dir__)

      desc "Install Turbo Desktop into your Rails application"

      def copy_initializer
        template "initializer.rb.tt", "config/initializers/turbo_desktop.rb"
      end

      def mount_engine
        route 'mount TurboDesktop::Engine => "/turbo-desktop"'
      end

      def show_next_steps
        say ""
        say "Turbo Desktop installed!", :green
        say ""
        say "Next steps:"
        say "  1. npx turbo-desktop init    # Scaffold the desktop shell"
        say "  2. rails server              # Start your Rails app"
        say "  3. npx turbo-desktop dev     # Launch the desktop app"
        say ""
      end
    end
  end
end
