# Skill: Compilacion y Despliegue Android Studio en Pixel 10 Pro (Arch Linux)
**Keywords:** android, kotlin, gradle, gradlew, apk, adb, pixel, noise-cat, noise cat, studio, 25.0.2

## 1. Diagnostico de Entorno en Arch Linux
- En esta estacion Arch Linux existen OpenJDK 25 y OpenJDK 21 (`/usr/lib/jvm/java-21-openjdk` y `/opt/android-studio/jbr`).
- **REGLA CRITICA CONTRA ERROR `25.0.2`:** Gradle 8.x falla con `* What went wrong: 25.0.2` si corre con Java 25. SIEMPRE debes fijar en `gradle.properties` la ruta exacta del JDK 21 o JBR:
`org.gradle.java.home=/opt/android-studio/jbr` (o `/usr/lib/jvm/java-21-openjdk` si jbr no existe).
- **REGLA CRITICA DE WRAPPER:** Nunca escribas `https\\://` con doble barra en `gradle-wrapper.properties`. Copia directamente `gradlew`, `gradle/wrapper/gradle-wrapper.jar` y `gradle/wrapper/gradle-wrapper.properties` desde `/home/daniel/AndroidStudioProjects/molex_v1`.
- **REGLA DE SDK:** Escanea `~/Android/Sdk/platforms/` para ver que API levels estan instalados. Si `compileSdk` no coincide, ajusta a lo disponible (ej: 34 o 35).

## 2. Compilacion
```bash
cd /ruta/proyecto
./gradlew --stop
./gradlew assembleDebug 2>&1 | tail -20
```
- APK resultante: `app/build/outputs/apk/debug/app-debug.apk`

## 3. Instalacion en Pixel 10 Pro
```bash
adb install -r app/build/outputs/apk/debug/app-debug.apk
```

## 4. Lanzamiento y Verificacion
- **REGLA CRITICA DE applicationId:** NO asumas `com.example.*`. Lee `app/build.gradle.kts` y extrae `applicationId` real. Para noise-cat es `com.bob.noisecat`.
```bash
adb shell am start -n com.bob.noisecat/.MainActivity
adb shell pidof com.bob.noisecat          # debe devolver PID
adb shell dumpsys window | grep mCurrentFocus  # debe mostrar com.bob.noisecat
```

## 5. Screenshot de Confirmacion
```bash
adb shell screencap -p /sdcard/check.png
adb pull /sdcard/check.png ./screenshot.png
```

## 6. Errores Comunes y Soluciones
| Error | Causa | Fix |
|-------|-------|-----|
| `25.0.2` | Java 25 con Gradle 8 | `org.gradle.java.home=/opt/android-studio/jbr` |
| `Could not find compileSdk 35` | Platform no instalada | Ajustar a 34 o instalar via SDK Manager |
| `Activity class does not exist` | applicationId equivocado | Leer build.gradle.kts para el ID real |
| `Permission denied` gradlew | Sin +x | `chmod +x gradlew` |
| `https\\://` en wrapper | Escape doble | Copiar wrapper desde molex_v1 |

## 7. Rutas Clave
- Proyecto noise-cat: `/home/daniel/proyectos/bob_workspaces/chat-1792430852639-93j0r/src/noise-cat/`
- JBR: `/opt/android-studio/jbr`
- SDK: `~/Android/Sdk/`
- ADB: `adb` (en PATH)
