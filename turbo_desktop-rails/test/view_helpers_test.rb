require "test_helper"

class ViewHelpersTestHost
  include TurboDesktop::ViewHelpers

  attr_reader :request

  def initialize(user_agent)
    @request = StubRequest.new(user_agent)
  end

  # Stub capture for turbo_desktop_only / turbo_web_only
  def capture(&block)
    block.call
  end

  # Minimal stub for Rails' tag.meta helper
  def tag
    @tag ||= Class.new do
      def meta(**attrs)
        flat = []
        attrs.each do |k, v|
          if k == :data && v.is_a?(Hash)
            v.each { |dk, dv| flat << %(data-#{dk.to_s.tr("_", "-")}="#{dv}") }
          else
            flat << %(#{k.to_s.tr("_", "-")}="#{v}")
          end
        end
        "<meta #{flat.join(" ")}>"
      end
    end.new
  end
end

class ViewHelpersTest < Minitest::Test
  DESKTOP_UA = "Turbo Desktop/0.1.0 (macOS; aarch64)"
  BROWSER_UA = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)"

  # --- turbo_desktop_app? (duplicated in ViewHelpers) ---

  def test_view_helper_detects_desktop_app
    host = ViewHelpersTestHost.new(DESKTOP_UA)
    assert host.turbo_desktop_app?
  end

  def test_view_helper_rejects_browser
    host = ViewHelpersTestHost.new(BROWSER_UA)
    refute host.turbo_desktop_app?
  end

  # --- turbo_desktop_platform ---

  def test_view_helper_platform_macos
    host = ViewHelpersTestHost.new(DESKTOP_UA)
    assert_equal "macos", host.turbo_desktop_platform
  end

  def test_view_helper_platform_windows
    host = ViewHelpersTestHost.new("Turbo Desktop/0.1.0 (Windows; x86_64)")
    assert_equal "windows", host.turbo_desktop_platform
  end

  def test_view_helper_platform_linux
    host = ViewHelpersTestHost.new("Turbo Desktop/0.1.0 (Linux; x86_64)")
    assert_equal "linux", host.turbo_desktop_platform
  end

  def test_view_helper_platform_nil_when_not_desktop
    host = ViewHelpersTestHost.new(BROWSER_UA)
    assert_nil host.turbo_desktop_platform
  end

  # --- turbo_desktop_arch ---

  def test_view_helper_arch_aarch64
    host = ViewHelpersTestHost.new(DESKTOP_UA)
    assert_equal "aarch64", host.turbo_desktop_arch
  end

  def test_view_helper_arch_x86_64
    host = ViewHelpersTestHost.new("Turbo Desktop/0.1.0 (macOS; x86_64)")
    assert_equal "x86_64", host.turbo_desktop_arch
  end

  def test_view_helper_arch_nil_when_not_desktop
    host = ViewHelpersTestHost.new(BROWSER_UA)
    assert_nil host.turbo_desktop_arch
  end

  # --- turbo_desktop_bridge ---

  def test_bridge_returns_hash_with_component_name
    host = ViewHelpersTestHost.new(DESKTOP_UA)
    result = host.turbo_desktop_bridge("menu-item")
    assert_equal "menu-item", result["data-turbo-desktop-bridge"]
  end

  def test_bridge_with_no_options
    host = ViewHelpersTestHost.new(DESKTOP_UA)
    result = host.turbo_desktop_bridge("notification")
    assert_equal({ "data-turbo-desktop-bridge" => "notification" }, result)
  end

  def test_bridge_with_options
    host = ViewHelpersTestHost.new(DESKTOP_UA)
    result = host.turbo_desktop_bridge("menu-item", title: "Export PDF", shortcut: "Cmd+E")

    assert_equal "menu-item", result["data-turbo-desktop-bridge"]
    assert_equal "Export PDF", result["data-turbo-desktop-bridge-title"]
    assert_equal "Cmd+E", result["data-turbo-desktop-bridge-shortcut"]
  end

  def test_bridge_converts_option_values_to_strings
    host = ViewHelpersTestHost.new(DESKTOP_UA)
    result = host.turbo_desktop_bridge("counter", count: 42, enabled: true)

    assert_equal "42", result["data-turbo-desktop-bridge-count"]
    assert_equal "true", result["data-turbo-desktop-bridge-enabled"]
  end

  def test_bridge_works_regardless_of_user_agent
    # Bridge helper does not depend on whether it is a desktop app
    host = ViewHelpersTestHost.new(BROWSER_UA)
    result = host.turbo_desktop_bridge("menu-item", title: "Test")
    assert_equal "menu-item", result["data-turbo-desktop-bridge"]
    assert_equal "Test", result["data-turbo-desktop-bridge-title"]
  end

  # --- turbo_desktop_only ---

  def test_desktop_only_renders_for_desktop
    host = ViewHelpersTestHost.new(DESKTOP_UA)
    result = host.turbo_desktop_only { "desktop content" }
    assert_equal "desktop content", result
  end

  def test_desktop_only_returns_nil_for_browser
    host = ViewHelpersTestHost.new(BROWSER_UA)
    result = host.turbo_desktop_only { "desktop content" }
    assert_nil result
  end

  # --- turbo_web_only ---

  def test_web_only_renders_for_browser
    host = ViewHelpersTestHost.new(BROWSER_UA)
    result = host.turbo_web_only { "web content" }
    assert_equal "web content", result
  end

  def test_web_only_returns_nil_for_desktop
    host = ViewHelpersTestHost.new(DESKTOP_UA)
    result = host.turbo_web_only { "web content" }
    assert_nil result
  end

  def test_inspector_predicate_reflects_config
    TurboDesktop.configuration.inspector_enabled = true
    host = ViewHelpersTestHost.new(DESKTOP_UA)
    assert host.turbo_desktop_inspector?
  ensure
    TurboDesktop.configuration.inspector_enabled = false
  end

  def test_inspector_meta_tag_present_when_enabled
    TurboDesktop.configuration.inspector_enabled = true
    host = ViewHelpersTestHost.new(DESKTOP_UA)
    assert_includes host.turbo_desktop_inspector_meta_tag.to_s, "turbo-desktop-inspector"
    assert_includes host.turbo_desktop_inspector_meta_tag.to_s, "enabled"
    # carries the same-origin inspector URL (fallback path outside a mounted app)
    assert_includes host.turbo_desktop_inspector_meta_tag.to_s, 'data-inspector-url="/turbo-desktop/inspector.js"'
  ensure
    TurboDesktop.configuration.inspector_enabled = false
  end

  def test_inspector_meta_tag_absent_when_disabled
    host = ViewHelpersTestHost.new(DESKTOP_UA)
    assert_nil host.turbo_desktop_inspector_meta_tag
  end
end
