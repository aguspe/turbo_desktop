require "test_helper"

# Uses the shared app + mounted engine from test_helper.rb.
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
