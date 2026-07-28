require "test_helper"

# The decision on its own.
class VariantForTest < Minitest::Test
  def variant_for(current: nil, desktop: true, configured: :desktop)
    TurboDesktop::Detection.variant_for(
      current: current, desktop: desktop, configured: configured
    )
  end

  def test_marks_desktop_requests
    assert_equal [ :desktop ], variant_for
  end

  def test_leaves_browser_requests_alone
    assert_nil variant_for(desktop: false)
  end

  def test_can_be_turned_off
    assert_nil variant_for(configured: nil)
    assert_nil variant_for(configured: "")
  end

  def test_keeps_variants_the_app_already_set
    # An app may be using variants for something else entirely.
    assert_equal [ :phone, :desktop ], variant_for(current: [ :phone ])
  end

  def test_does_not_add_itself_twice
    assert_nil variant_for(current: [ :desktop ])
    assert_nil variant_for(current: [ "desktop" ])
  end

  def test_honours_a_custom_variant_name
    assert_equal [ :turbo_desktop ], variant_for(configured: :turbo_desktop)
  end
end

# The same thing through a real request.
class VariantRequestTest < ActionController::TestCase
  class VariantsController < ActionController::Base
    def show
      render plain: request.variant.to_a.join(",")
    end
  end

  tests VariantsController

  DESKTOP_UA = "Turbo Desktop/0.1.1 (macOS; aarch64)".freeze
  BROWSER_UA = "Mozilla/5.0 (Macintosh) AppleWebKit/605.1.15 Safari/605.1.15".freeze

  # An isolated route set, so the shared application's routes are left alone.
  setup do
    @routes = ActionDispatch::Routing::RouteSet.new
    @routes.draw { get ":action", controller: "variant_request_test/variants" }
  end

  def show_with(user_agent)
    request.headers["User-Agent"] = user_agent
    get :show
    response.body
  end

  def test_desktop_requests_get_the_variant
    assert_equal "desktop", show_with(DESKTOP_UA)
  end

  def test_browser_requests_do_not
    assert_equal "", show_with(BROWSER_UA)
  end

  def test_the_variant_name_is_configurable
    TurboDesktop.configure { |config| config.variant = :native_shell }

    assert_equal "native_shell", show_with(DESKTOP_UA)
  end

  def test_it_can_be_disabled
    TurboDesktop.configure { |config| config.variant = nil }

    assert_equal "", show_with(DESKTOP_UA)
  end
end
