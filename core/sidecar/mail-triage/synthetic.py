"""Fichas SINTÉTICAS (dominios example.*, datos inventados) con etiqueta a mano para smoke.py y eval.py.

Las señales de `deterministic` las escribí yo a mano; no salen del núcleo.
"""

from typing import NamedTuple

PREFS = "Quiere facturas y avisos de seguridad; no quiere ofertas ni boletines."


def mk(addr, subject, preview, *, name="", auth=("pass", "pass", "pass"), reply_to=None,
       links=0, contact=False, unsub=False, link_host_differs=False, level="normal", signals=()):
    domain = addr.split("@")[1]
    reply_domain = reply_to.split("@")[1] if reply_to else None
    return {
        "from_name": name,
        "from_address": addr,
        "subject": subject,
        "preview": preview,
        "checks": {
            "from_domain": domain,
            "reply_to_domain": reply_domain,
            "return_path_domain": domain,
            "reply_to_differs": reply_domain is not None and reply_domain != domain,
            "return_path_differs": False,
            "auth": dict(zip(("spf", "dkim", "dmarc"), auth)),
            "claims_attachment_but_none": False,
            "link_host_differs": link_host_differs,
            "list_unsubscribe": unsub,
            "one_click_unsubscribe": unsub,
            "bulk": unsub,
        },
        "deterministic": {"level": level, "signals": list(signals)},
        "link_count": links,
        "contact_known": contact,
        "preferences": PREFS,
    }


class Case(NamedTuple):
    id: str
    ficha: dict
    kind: str
    phish: bool
    action: str


def _phish(cid, addr, subject, preview, **kw):
    kw.setdefault("level", "sospechoso")
    return Case(cid, mk(addr, subject, preview, **kw), "estafa_phishing", True, "eliminar")


def _legit(cid, kind, action, addr, subject, preview, **kw):
    return Case(cid, mk(addr, subject, preview, **kw), kind, False, action)


# 12 phishing (banco, paquete, sorteo, soporte falso, factura falsa, contraseña falsa, CEO fraud).
PHISHING = [
    _phish("phish_banco_es", "alertas@banco-seguro.example", "Su cuenta ha sido limitada: verifíquela hoy",
           "Para evitar el bloqueo, confirme su número de tarjeta y su clave en el enlace.",
           name="Banco Seguro", auth=("fail", "fail", "fail"), reply_to="soporte@mail-rapido.example",
           links=1, link_host_differs=True, signals=("dmarc_fail", "reply_to_differs")),
    _phish("phish_banco_en", "security@mybank-online.example", "Unusual activity on your card",
           "We blocked a payment. Confirm your card PIN here within 2 hours to keep your card active.",
           name="MyBank Security", auth=("fail", "pass", "fail"), links=1, link_host_differs=True,
           signals=("dmarc_fail", "enlace_otro_host")),
    _phish("phish_paquete_es", "seguimiento@entregas-express.example", "Su paquete está retenido: pague 2,49 EUR",
           "Hemos retenido su envío por una tasa pendiente. Pague en el enlace para liberarlo.",
           name="Entregas Express", auth=("fail", "fail", "fail"), links=1, link_host_differs=True,
           signals=("dmarc_fail",)),
    _phish("phish_paquete_en", "delivery@parcel-track.example", "Missed delivery: reschedule your parcel",
           "Your parcel could not be delivered. Pay the 1.99 GBP redelivery fee to reschedule.",
           name="Parcel Track", auth=("fail", "none", "fail"), links=1, link_host_differs=True,
           signals=("dmarc_fail",)),
    _phish("phish_sorteo_es", "premios@loteria-ganadora.example", "¡Enhorabuena! Ha ganado un iPhone",
           "Reclame su premio en 24 horas. Envíenos sus datos bancarios para el envío.",
           name="Lotería Premio", auth=("fail", "none", "fail"), links=2, signals=("dmarc_fail",)),
    _phish("phish_sorteo_en", "claims@giftcard-winners.example", "You have won a 500 USD gift card",
           "Claim within 12 hours. Enter your login and bank details on the claim page.",
           name="Gift Winners", auth=("fail", "fail", "fail"), links=2, signals=("dmarc_fail",)),
    _phish("phish_soporte_es", "soporte@apple-ayuda.example", "Alerta de seguridad: su equipo tiene un virus",
           "Llame ya al número indicado y permita el acceso remoto para eliminar la amenaza.",
           name="Soporte Apple", auth=("fail", "fail", "fail"), reply_to="tecnicos@reparacion-rapida.example",
           signals=("dmarc_fail", "reply_to_differs")),
    _phish("phish_soporte_en", "support@microsoft-help-desk.example", "Your computer is infected: call us now",
           "Our system detected a virus. Call the number below and grant remote access to fix it.",
           name="Microsoft Support", auth=("fail", "fail", "fail"), signals=("dmarc_fail",)),
    _phish("phish_factura_es", "facturacion@proveedor-pagos.example", "Factura pendiente nº 88213: pago urgente",
           "Si no paga hoy se aplicarán recargos. Transfiera al nuevo IBAN indicado en el enlace.",
           name="Facturación Proveedor", auth=("fail", "pass", "fail"), links=1, link_host_differs=True,
           signals=("dmarc_fail", "cambio_iban")),
    _phish("phish_factura_en", "accounts@vendor-billing.example", "Overdue invoice 4471: payment needed today",
           "Pay the amount using the updated bank details below to avoid suspension.",
           name="Vendor Billing", auth=("fail", "fail", "fail"), links=1, link_host_differs=True,
           signals=("dmarc_fail",)),
    _phish("phish_contrasena_en", "noreply@mail-security-reset.example", "Your password expires today",
           "Click here to keep your current password. Enter your username and password to confirm.",
           name="IT Helpdesk", auth=("fail", "none", "fail"), links=1, link_host_differs=True,
           signals=("dmarc_fail", "enlace_otro_host")),
    _phish("phish_ceo_es", "director.general@empresa-ejemplo.example", "Urgente y confidencial: transferencia de hoy",
           "Necesito que hagas hoy una transferencia a este proveedor. Es confidencial: no llames, responde por correo.",
           name="Director General", auth=("pass", "fail", "fail"),
           reply_to="director.general@empresa-ejemplo-mail.example", signals=("reply_to_differs",)),
]

# 28 legítimos difíciles: facturas, avisos de seguridad, boletines, marketing de una tienda, correo personal,
# calendario, recibos, envíos, y tono urgente legítimo.
LEGITIMATE = [
    _legit("factura_real_es", "transaccional", "conservar", "facturacion@proveedor.example",
           "Tu factura de septiembre ya está disponible", "Puedes descargarla desde tu área de cliente. Importe: 19,90 EUR.",
           name="Proveedor Ejemplo", links=1, contact=True),
    _legit("factura_real_en", "transaccional", "conservar", "billing@saas.example",
           "Invoice INV-2291 for your September plan", "Thanks for your payment. Your invoice is available in your account.",
           name="SaaS Example", links=1, contact=True),
    _legit("seguridad_real_es", "transaccional", "conservar", "no-reply@cuenta.example",
           "Nuevo inicio de sesión en tu cuenta",
           "Se ha iniciado sesión desde un dispositivo nuevo en Madrid. Si fuiste tú, no tienes que hacer nada.",
           name="Cuenta Ejemplo", links=1, contact=True),
    _legit("seguridad_real_en", "transaccional", "conservar", "no-reply@accounts.example",
           "Your password was changed",
           "The password for your account was changed. If you did not make this change, contact support from the settings page.",
           name="Accounts Example", contact=True),
    _legit("banco_real_es", "transaccional", "conservar", "avisos@banco.example",
           "Has recibido una transferencia de 120 EUR",
           "Hemos abonado 120,00 EUR en tu cuenta. Consulta el movimiento en la banca online.",
           name="Banco Ejemplo", contact=True),
    _legit("boletin_es", "boletin_suscrito", "no_deseado", "novedades@tienda.example",
           "Novedades de octubre: nuevas colecciones",
           "Descubre las colecciones de otoño y los artículos más vendidos del mes.",
           name="Tienda Ejemplo", links=3, unsub=True),
    _legit("boletin_en", "boletin_suscrito", "no_deseado", "news@devtips.example",
           "Weekly digest: 5 articles on Python",
           "This week: packaging tips, async patterns and a new runtime release.",
           name="DevTips", links=4, unsub=True),
    _legit("boletin_urgente_en", "boletin_suscrito", "revisar", "news@sportsclub.example",
           "Last hours to renew at the early-bird price",
           "Early-bird renewal closes tonight at midnight. Renew now to keep your current rate.",
           name="Sports Club", links=1, unsub=True),
    _legit("boletin_urgente_es", "boletin_suscrito", "revisar", "novedades@club-lector.example",
           "Últimas horas: cierra la inscripción al club de lectura",
           "Hoy es el último día para apuntarte con el precio de socio. Entra y reserva tu plaza.",
           name="Club Lector", links=1, unsub=True),
    _legit("marketing_tienda_es", "marketing", "no_deseado", "ofertas@tienda.example",
           "Un 20% de descuento en tu próxima compra",
           "Como cliente, tienes un 20% de descuento en tu siguiente pedido hasta el domingo.",
           name="Tienda Ejemplo", links=2, unsub=True),
    _legit("marketing_tienda_en", "marketing", "no_deseado", "deals@shop.example",
           "Thanks for shopping with us: 15% off accessories",
           "As a past customer, get 15% off accessories until Sunday.",
           name="Shop Example", links=2, unsub=True),
    _legit("marketing_urgente_es", "marketing", "no_deseado", "ofertas@tiendaxl-ejemplo.example",
           "Solo este fin de semana: rebajas de otoño",
           "Hasta el domingo a medianoche: rebajas en abrigos y botas. Aprovecha antes de que se agoten.",
           name="TiendaXL Ejemplo", links=2, unsub=True),
    _legit("personal_es", "personal", "conservar", "ana.perez@correo.example",
           "¿Nos vemos el jueves? Te traigo los apuntes",
           "Dime si te va bien a las seis en la biblioteca. Un abrazo.", name="Ana Pérez", contact=True),
    _legit("personal_en", "personal", "conservar", "tom.r@mail.example",
           "Photos from the trip", "Here are the photos from Lisbon. See you at dinner on Friday.",
           name="Tom", contact=True),
    _legit("personal_enlace_es", "personal", "conservar", "mama.ejemplo@correo.example",
           "Mira las fotos de la boda", "Te paso el enlace del álbum, lo he subido a mi nube. Besos.",
           name="Mamá", links=1, contact=True),
    _legit("personal_urgente_en", "personal", "conservar", "jess.k@mail.example",
           "Call me ASAP, it's about the flat", "The landlord just called. Can you call me back today? It's important.",
           name="Jess", contact=True),
    _legit("calendario_es", "transaccional", "conservar", "invitaciones@calendario.example",
           "Invitación: Revisión del proyecto, jueves 10:00",
           "Ana te ha invitado a «Revisión del proyecto». Responde para confirmar tu asistencia.",
           name="Calendario", links=1, contact=True),
    _legit("calendario_en", "transaccional", "conservar", "calendar-noreply@calendar.example",
           "Reminder: dentist appointment tomorrow at 9:00",
           "This is a reminder for your appointment with Clinic Example. Reply to reschedule.",
           name="Calendar"),
    _legit("recibo_es", "transaccional", "conservar", "recibos@tienda.example",
           "Tu recibo de compra nº 55120",
           "Gracias por tu compra. Total: 34,50 EUR. Puedes ver el detalle en tu cuenta.",
           name="Tienda Ejemplo", links=1, contact=True),
    _legit("recibo_en", "transaccional", "conservar", "receipts@cafe.example",
           "Your receipt from Cafe Example", "Thanks for visiting. Total paid: 8.40 USD. Card ending 1234.",
           name="Cafe Example"),
    _legit("envio_es", "transaccional", "conservar", "seguimiento@mensajeria.example",
           "Tu paquete sale hoy de nuestro almacén",
           "Tu pedido 7731 está en reparto. Puedes seguir el envío desde el enlace de tu cuenta.",
           name="Mensajería Ejemplo", links=1, contact=True),
    _legit("envio_en", "transaccional", "conservar", "tracking@parcel-service.example",
           "Your parcel has been shipped",
           "Your order is on its way. Track it in your account; expected delivery Thursday.",
           name="Parcel Service", links=1),
    _legit("reset_pedido_es", "transaccional", "conservar", "no-reply@cuenta.example",
           "Has pedido restablecer tu contraseña",
           "Si fuiste tú, usa el enlace para elegir una nueva contraseña. El enlace caduca en 1 hora.",
           name="Cuenta Ejemplo", links=1, contact=True),
    _legit("cargo_en", "transaccional", "conservar", "alerts@bank.example",
           "Card payment of 12.00 USD at Example Coffee",
           "A card payment was made. If you don't recognise it, sign in to the app to report it.",
           name="Bank Example", contact=True),
    _legit("reclutador_en", "otro", "revisar", "recruiter@company.example",
           "Following up on your application",
           "Thanks for applying. Could you share two dates for a call next week?", name="Company Recruiting"),
    _legit("vecinos_es", "otro", "conservar", "junta@vecinos.example",
           "Convocatoria de la junta de vecinos",
           "Os convoco el martes a las 19:00 en el portal. Orden del día adjunto.", name="Junta de Vecinos"),
    _legit("tarjeta_caduca_es", "transaccional", "conservar", "avisos@banco.example",
           "Tu tarjeta caduca el 31 de octubre",
           "Te enviaremos una nueva tarjeta sin coste. No necesitas hacer nada. Consulta más en tu banca online.",
           name="Banco Ejemplo", links=1, contact=True),
    _legit("encuesta_en", "transaccional", "revisar", "feedback@shop.example",
           "How was your order? Leave a review in 2 minutes",
           "Tell us what you think about your recent order. It only takes two minutes.",
           name="Shop Example", links=1, contact=True),
]

LABELED = PHISHING + LEGITIMATE
