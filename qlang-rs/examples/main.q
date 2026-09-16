rem Exemple d'utilisation de "inclure" pour charger plusieurs fichiers modules.
inclure "modules/math.q"
inclure "modules/greetings.q"
inclure "modules/tableaux.q"
inclure "modules/objets.q"

dec nombre = 6
ecrire saluer("Quentin")
ecrire "Le carré de " + nombre + " est " + carre(nombre)
ecrire "Le double de " + nombre + " est " + double(nombre)

dec valeurs = [1, 2, 3, 4, 5]
ecrire "La somme de " + valeurs + " est " + somme(valeurs)

dec nom = lire "Quel est ton nom ?"
ecrire saluer(nom)

dec auteur = Personnage.nouveau(nom)
ecrire auteur.lire_nom() + " a " + auteur.lire_vie() + " de vie"
