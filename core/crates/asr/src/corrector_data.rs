//! The closed lexicon the corrector maps to, and the common words it protects. Both are copied from
//! `research/voz-spike/bench/lexico.py` and `palabras_comunes.py`: changing them means measuring again.

/// Command verbs (imperative and with clitics), then the names and nouns of the product.
pub(crate) const LEXICON: &[&str] = &[
    "borra",
    "borrar",
    "aprueba",
    "apruébalo",
    "deniega",
    "deniégalo",
    "delega",
    "delégalo",
    "anota",
    "recuérdame",
    "avísame",
    "escríbele",
    "mueve",
    "crea",
    "prepara",
    "envía",
    "envíalo",
    "cancela",
    "guarda",
    "busca",
    "abre",
    "cierra",
    "repite",
    "para",
    "sigue",
    "excubia",
    "alfred",
    "borrador",
    "presupuesto",
    "calendario",
    "contacto",
    "nota",
    "aprobación",
    "equipo",
    "cliente",
    "proveedor",
    "proyecto",
];

/// Frequent Spanish words and the words of the script, one entry per plain form (no accents, no
/// case). The corrector does not change them, so «borré la repetida» stays as it is.
pub(crate) const COMMON_WORDS: &str = "
de la que el en y a los del se las por un para con no una su al lo como más pero sus le ya o
este sí porque esta entre cuando muy sin sobre también me hasta hay donde quien desde todo nos
durante todos uno les ni contra otros ese eso ante ellos e esto mí antes algunos unos yo otro
otras otra tanto esa estos mucho quienes nada muchos cual poco ella estar estas algunas algo
nosotros mis tú te ti tus ellas nosotras vosotros vosotras os mío mía míos mías tuyo tuya suyo
suya nuestro nuestra vuestro vuestra es son ser soy eres somos sois fue fueron era eran sido
siendo he has ha hemos habéis han había hecho hacer hago hace hacen tiene tienen tener tengo
tienes tenemos tenía tenían ir voy vas va vamos van iba ido dar doy das dan da dio decir digo
dice dicen decía dije poder puedo puede pueden podemos podía quiero quieres quiere queremos
querer ver veo ves ve vemos visto saber sabe sabes saben sabemos haber estoy estamos están
estaba estaban estado hoy ayer mañana noche día días tarde mes años año semana vez veces cada
mismo misma buen bueno buena buenos buenas mejor mal malo grande gran pequeño nuevo nueva nuevos
viejo primero primera último última mucha muchas pocos pocas tan tal además aquí ahí allí allá
ahora luego después siempre nunca jamás aún todavía quizás acaso bien así sólo solamente incluso
hacia según mediante tras sino aunque mientras entonces pues cuáles cuánto cuánta cuántos
cuántas gracias hola adiós vale claro oye oiga eh ah oh vaya venga anda perdón favor ok genial
perfecto correcto cosa cosas parte caso forma modo tipo lugar vida hombre mujer persona personas
gente hijo hija amigo amiga señor señora trabajo trabajar trabajando trabajan borré borró
borraste borrado pedir pido pides pide pidió pedí pedido pedidos llegar llegó llegado llegaron
llega llegan llamar llamo llamé llama llamada llamadas hablar hablo habla hablé hablamos dejar
dejo deja dejé dejalo pasar paso pasa pasado pasan poner pongo pone puse puesto poned quedar
queda quedo quedé quedan seguir sigo siguen seguido empezar empiezo empieza empecé terminar
termino termina terminé acabar acabo acaba acabé llevar llevo lleva llevé traer traigo trae usar
uso usa usé encontrar encuentro encuentra encontré buscar busco busca hizo mirar miro mira miré
salir salgo sale salí entrar entro entra volver vuelvo vuelve volví pensar pienso piensa creer
creo cree creí sentir siento siente sentí parecer parece dame dime decirme escuchar escucho
escucha escuché escucharte leer leo lee leí escribir escribo escribe escribí comer mandar mando
manda mandé enviar envío envía envié recibir recibo recibe recibí mostrar muestro muestra mostré
conocer conozco conoce conocí necesitar necesito necesita necesité necesitamos ayudar ayudo
ayuda ayudé ayúdame gustar gusta gustan importar importa importante importantes problema
problemas idea ideas dato datos cifra cifras número números total totales precio precios dinero
euro euros céntimo céntimos impuestos importe cuenta cuentas correo correos mensaje mensajes
nombre nombres lista listas archivo archivos documento documentos página páginas fecha hora
horas minuto minutos segundo segundos momento rato ratos cerca lejos dentro fuera arriba abajo
delante detrás encima debajo izquierda derecha oficina casa coche ciudad país calle mañanas
lunes martes miércoles jueves viernes sábado domingo enero febrero marzo abril mayo junio julio
agosto septiembre octubre noviembre diciembre dos tres cuatro cinco seis siete ocho nueve diez
once doce veinte treinta cuarenta cincuenta cien mil primer tercero medio media doble resto
demás activa adelante agenda agente agentes alfred alguna anota anotamos aplicación aprobaciones
aprobación apruébalo arroba avísame añade banco borra borrador cambia cancela cliente com
contacto contactos conversación crea cuatrocientos delega delegación deniégalo despacio dile
doscientos ejemplo equipo ernesto escríbele espera estricto excubia explícamelo guarda haciendo
incompleto lópez madrid marta mueve mándame nota oyes pendiente plazo prefiere pregúntale
prepara presupuesto proveedor proyecto pruebas punto recuérdame repetida resume resumen reunión
revise título
";
