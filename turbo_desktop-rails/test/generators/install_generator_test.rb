require "bundler/setup"
require "minitest/autorun"
require "rails/generators"
require "rails/generators/test_case"

# Load the generator
require_relative "../../lib/generators/turbo_desktop/install/install_generator"

class InstallGeneratorTest < Rails::Generators::TestCase
  tests TurboDesktop::Generators::InstallGenerator
  destination File.expand_path("../tmp", __dir__)

  setup do
    prepare_destination
    # Create a minimal routes.rb for the route injection
    FileUtils.mkdir_p(File.join(destination_root, "config"))
    File.write(
      File.join(destination_root, "config", "routes.rb"),
      "Rails.application.routes.draw do\nend\n"
    )
  end

  test "creates initializer file" do
    run_generator
    assert_file "config/initializers/turbo_desktop.rb"
  end

  test "initializer contains TurboDesktop.configure block" do
    run_generator
    assert_file "config/initializers/turbo_desktop.rb", /TurboDesktop\.configure do \|config\|/
  end

  test "initializer contains path_configuration" do
    run_generator
    assert_file "config/initializers/turbo_desktop.rb", /config\.path_configuration/
  end

  test "initializer contains all presentation types in comments" do
    run_generator
    assert_file "config/initializers/turbo_desktop.rb" do |content|
      assert_match(/"default"/, content)
      assert_match(/"modal"/, content)
      assert_match(/"new_window"/, content)
      assert_match(/"replace"/, content)
      assert_match(/"native"/, content)
      assert_match(/"none"/, content)
    end
  end

  test "mounts engine in routes.rb" do
    run_generator
    assert_file "config/routes.rb", /mount TurboDesktop::Engine => "\/turbo-desktop"/
  end

  test "does not duplicate route on second run" do
    run_generator
    run_generator
    assert_file "config/routes.rb" do |content|
      matches = content.scan(/mount TurboDesktop::Engine/)
      assert_equal 1, matches.length, "Route should appear exactly once"
    end
  end

  test "initializer contains default rules" do
    run_generator
    assert_file "config/initializers/turbo_desktop.rb" do |content|
      assert_match(/patterns: \["\/"\]/, content)
      assert_match(/patterns: \["\/new\$", "\/edit\$"\]/, content)
      assert_match(/screenshots_enabled: false/, content)
    end
  end
end
