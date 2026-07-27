require "test_helper"
require "action_controller"
require "action_dispatch"

# Build a minimal Rails app to test the controller in isolation
require "rails"

class TestApp < Rails::Application
  config.eager_load = false
  config.secret_key_base = "test-secret-key-base-for-turbo-desktop-tests"
  config.hosts.clear
end

# Load the engine
require "turbo_desktop/engine"

TestApp.initialize!

# Draw routes
Rails.application.routes.draw do
  mount TurboDesktop::Engine => "/turbo-desktop"
end

class PathConfigurationsControllerTest < ActionDispatch::IntegrationTest
  def test_show_returns_json
    get "/turbo-desktop/path-configuration.json"
    assert_response :success
    assert_equal "application/json", response.media_type
  end

  def test_show_returns_default_path_configuration
    get "/turbo-desktop/path-configuration.json"
    body = JSON.parse(response.body)

    assert body.key?("settings")
    assert body.key?("rules")
    assert_equal false, body["settings"]["screenshots_enabled"]
    assert_equal 1, body["rules"].length
    assert_equal [ "/" ], body["rules"].first["patterns"]
    assert_equal "default", body["rules"].first["properties"]["presentation"]
  end

  def test_show_returns_custom_path_configuration
    TurboDesktop.configure do |config|
      config.path_configuration = {
        settings: { screenshots_enabled: true },
        rules: [
          { patterns: [ "/new", "/edit" ], properties: { presentation: "modal" } },
          { patterns: [ "/" ], properties: { presentation: "default" } }
        ]
      }
    end

    get "/turbo-desktop/path-configuration.json"
    body = JSON.parse(response.body)

    assert_equal true, body["settings"]["screenshots_enabled"]
    assert_equal 2, body["rules"].length
    assert_equal "modal", body["rules"].first["properties"]["presentation"]
  end

  def test_show_responds_to_json_format
    get "/turbo-desktop/path-configuration", headers: { "Accept" => "application/json" }
    assert_response :success
  end
end
