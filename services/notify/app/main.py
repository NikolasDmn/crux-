from app.http.routes import internal, router
from app.settings import get_settings
from crux_common.app import create_app

app = create_app(get_settings(), "Crux notify", [internal, router])
