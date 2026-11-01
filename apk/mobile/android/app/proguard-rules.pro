# WorkManager (reminders) builds its Room database by reflection; R8 would drop the constructor
# and the app dies at start with NoSuchMethodException (seen in the 1.0.2/1.0.3 releases).
-keep class * extends androidx.room.RoomDatabase { <init>(); }
-keep class androidx.work.impl.WorkDatabase_Impl { <init>(); }
