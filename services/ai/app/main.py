from app.http.routes import router
from app.settings import get_settings
from crux_common.app import create_app

app = create_app(get_settings(), "Crux AI", [router])
