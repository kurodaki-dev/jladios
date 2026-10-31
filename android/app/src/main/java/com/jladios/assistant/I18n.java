package com.jladios.assistant;

/** Textes de l'interface Android : français ou anglais selon /lang. */
final class I18n {
    private I18n() {}

    static boolean fr() {
        return !"en".equals(Native.uiLang());
    }

    static String t(String fr, String en) {
        return fr() ? fr : en;
    }
}
