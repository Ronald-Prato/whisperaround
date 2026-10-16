# Seguridad

Este documento es para quien quiera auditar qué hace WhisperAround con tu API
key de OpenAI y con tu audio antes de usarla. Todo lo que se afirma aquí apunta
al código que lo implementa.

## La API key de OpenAI

| Pregunta | Respuesta | Dónde verlo |
| --- | --- | --- |
| ¿A dónde se envía? | Solo a `https://api.openai.com/v1/audio/transcriptions`, como cabecera `Authorization: Bearer`, cuando transcribes con un modelo de OpenAI. | [`src-tauri/src/whisper.rs`](src-tauri/src/whisper.rs) (único uso de la key; test `key_goes_only_to_openai_over_https`) |
| ¿Dónde se guarda? | Windows: Administrador de credenciales (cifrado con DPAPI, ligado a tu usuario). macOS: Llavero. Linux: archivo de ajustes de la app. Nunca en `settings.json` en Windows/macOS. | [`src-tauri/src/secret.rs`](src-tauri/src/secret.rs), [`src-tauri/src/settings.rs`](src-tauri/src/settings.rs) (test `key_stays_out_of_the_file_when_the_os_store_exists`) |
| ¿La ve la interfaz (webview)? | No. El frontend solo recibe `apiKeyConfigured: true/false`; la key nunca vuelve del proceso Rust. | `SettingsView` en [`settings.rs`](src-tauri/src/settings.rs) (test `view_never_contains_the_key`) |
| ¿Se registra en logs? | No. Los errores de OpenAI se muestran tal cual los devuelve la API (que enmascara la key). | [`whisper.rs`](src-tauri/src/whisper.rs) |

Las versiones anteriores a la 0.1.5 guardaban la key en `settings.json`; al
abrir la 0.1.5 se mueve automáticamente al llavero y se borra del archivo.

**Recomendación:** crea en OpenAI una API key exclusiva para WhisperAround, con
límite de gasto. Si la revocas, no afecta a nada más.

## Conexiones de red

La app no tiene telemetría, analíticas ni servidores propios. Estas son todas
las conexiones que hace:

| Destino | Cuándo | Qué se envía |
| --- | --- | --- |
| `api.openai.com` | Al transcribir con un modelo de OpenAI | El audio grabado (WAV) y tu API key |
| `pypi.org` | La primera vez que usas un modelo local | Descarga del paquete `faster-whisper` en un entorno privado de la app |
| `huggingface.co` | La primera vez que usas cada modelo local | Descarga del modelo |
| `github.com` | Al abrir la app | Consulta si hay versión nueva (`latest.json`); no envía datos tuyos |

Con un modelo local, el audio no sale de tu equipo.

Además, la interfaz corre con una Content Security Policy estricta
([`tauri.conf.json`](src-tauri/tauri.conf.json)): no puede cargar scripts ni
conectarse a ningún sitio externo, solo hablar con el proceso Rust de la app.

## Permisos que usa y por qué

- **Micrófono:** grabar mientras dictas (solo entre pulsar y volver a pulsar el
  atajo).
- **Portapapeles + simular `Ctrl+V`:** pegar el texto transcrito donde está el
  cursor (se puede desactivar en Ajustes).
- **Atajo global de teclado:** empezar/terminar el dictado desde cualquier app.
  Solo se registra ese atajo; la app no lee el resto del teclado.

## Verificar que un instalador viene de este código

Los instaladores se compilan en GitHub Actions
([`release.yml`](.github/workflows/release.yml)) a partir del tag publicado,
y cada uno lleva una atestación de procedencia firmada por GitHub:

```bash
gh attestation verify WhisperAround_0.1.5_x64-setup.exe -R dansaround/whisperaround
```

Las actualizaciones automáticas solo se instalan si su firma coincide con la
clave pública incluida en la app.

## Reportar una vulnerabilidad

Abre un [aviso de seguridad privado](https://github.com/dansaround/whisperaround/security/advisories/new)
en GitHub (no un issue público).
