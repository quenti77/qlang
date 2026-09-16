structure Personnage avec
    cacher nom
    cacher vie = 100
fin

dans Personnage implemente
    publique nouveau(nom)
        dec perso = Personnage()
        perso.nom = nom
        retour perso
    fin

    publique lire_nom(moi)
        retour moi.nom
    fin

    publique lire_vie(moi)
        retour moi.vie
    fin
fin
